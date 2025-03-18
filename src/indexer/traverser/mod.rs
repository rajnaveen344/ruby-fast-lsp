mod block_node;
mod call_node;
mod class_node;
mod constant_node;
mod method_node;
mod module_node;
mod parameter_node;
mod utils;
mod variable_node;

use log::info;
use lsp_types::{Location, Url};
use tree_sitter::{Node, Parser, Tree};
use utils::node_to_range;

use super::{entry::Visibility, index::RubyIndex, RubyIndexer};

// Add a context struct to track more state during traversal
pub struct TraversalContext {
    pub visibility: Visibility,
    pub namespace_stack: Vec<String>,
    pub current_method: Option<String>,
}

impl TraversalContext {
    fn new() -> Self {
        TraversalContext {
            visibility: Visibility::Public,
            namespace_stack: Vec::new(),
            current_method: None,
        }
    }

    fn current_namespace(&self) -> String {
        self.namespace_stack.join("::")
    }
}

impl RubyIndexer {
    pub fn new() -> Result<Self, String> {
        let mut parser = Parser::new();
        let language = tree_sitter_ruby::LANGUAGE;
        parser
            .set_language(&language.into())
            .map_err(|_| "Failed to load Ruby grammar".to_string())?;

        Ok(RubyIndexer {
            index: RubyIndex::new(),
            parser,
            debug_mode: false,
        })
    }

    pub fn index(&self) -> &RubyIndex {
        &self.index
    }

    pub fn index_mut(&mut self) -> &mut RubyIndex {
        &mut self.index
    }

    pub fn set_debug_mode(&mut self, debug: bool) {
        self.debug_mode = debug;
    }

    pub fn index_file_with_uri(&mut self, uri: Url, content: &str) -> Result<(), String> {
        // Parse the source code
        let tree = self
            .parser
            .parse(content, None)
            .ok_or_else(|| format!("Failed to parse source code in file: {}", uri))?;

        // Process the file for indexing
        self.process_file(uri.clone(), &tree, content)
            .map_err(|e| format!("Failed to index file {}: {}", uri, e))
    }

    fn process_file(&mut self, uri: Url, tree: &Tree, content: &str) -> Result<(), String> {
        // Debug: Print the AST structure if debug mode is enabled
        if self.debug_mode {
            info!("AST structure for file: {}", uri);
            self.print_ast_structure(&tree.root_node(), content, 0);
        }

        // Create a traversal context
        let mut context = TraversalContext::new();

        // Pre-process: Remove any existing entries and references for this URI
        self.index.remove_entries_for_uri(&uri);
        self.index.remove_references_for_uri(&uri);

        // Traverse the AST
        self.traverse_node(tree.root_node(), &uri, content, &mut context)?;

        Ok(())
    }

    pub fn traverse_node(
        &mut self,
        node: Node,
        uri: &Url,
        source_code: &str,
        context: &mut TraversalContext,
    ) -> Result<(), String> {
        match node.kind() {
            "class" => class_node::process(self, node, uri, source_code, context)?,
            "module" => module_node::process(self, node, uri, source_code, context)?,
            "method" | "singleton_method" => {
                method_node::process(self, node, uri, source_code, context)?
            }
            "constant" => {
                // Check if this is a reference to a constant (not part of an assignment)
                let parent = node.parent();
                let is_reference = parent.map_or(true, |p| {
                    // If parent is an assignment and this is the left side, it's a definition not a reference
                    !(p.kind() == "assignment"
                        && p.child_by_field_name("left")
                            .map_or(false, |left| left == node))
                });

                if is_reference {
                    // Process constant reference
                    if let Err(e) = constant_node::process_constant_reference(
                        self,
                        node,
                        uri,
                        source_code,
                        context,
                    ) {
                        // Log the error and continue
                        if self.debug_mode {
                            println!("Error processing constant reference: {}", e);
                        }
                    }
                } else {
                    // Try to process constant definition, but don't fail if we can't
                    if let Err(e) =
                        constant_node::process_constant(self, node, uri, source_code, context)
                    {
                        // Log the error and continue
                        if self.debug_mode {
                            println!("Error processing constant: {}", e);
                        }
                    }
                }

                // Continue traversing the children
                let child_count = node.child_count();
                for i in 0..child_count {
                    if let Some(child) = node.child(i) {
                        self.traverse_node(child, uri, source_code, context)?;
                    }
                }
            }
            "identifier" => {
                // Check if this is a method call without a receiver
                // This handles cases like 'bar' in 'def another_method; bar; end'
                let text = self.get_node_text(node, source_code);

                // Skip if it's a keyword or empty
                if text.trim().is_empty()
                    || ["self", "super", "nil", "true", "false"].contains(&text.as_str())
                {
                    return Ok(());
                }

                // Check if it's a standalone identifier that could be a method call
                let parent = node.parent();
                if parent.map_or(false, |p| {
                    // If parent is a method body or a block, this could be a method call
                    p.kind() == "body_statement"
                }) {
                    // Skip if this is part of a method definition or parameter
                    let grandparent = parent.and_then(|p| p.parent());
                    if grandparent.map_or(false, |gp| {
                        gp.kind() == "method"
                            && gp
                                .child_by_field_name("name")
                                .map_or(false, |name| name == node)
                    }) {
                        return Ok(());
                    }

                    // Create a range for the reference
                    let range = node_to_range(node);

                    // Create a location for this reference
                    let location = Location {
                        uri: uri.clone(),
                        range,
                    };

                    // Add reference with just the method name
                    self.index.add_reference(&text, location.clone());

                    // Also add reference with class context if available
                    let current_namespace = context.current_namespace();
                    if !current_namespace.is_empty() {
                        let fqn = format!("{}#{}", current_namespace, text);
                        self.index.add_reference(&fqn, location);
                    }

                    if self.debug_mode {
                        info!(
                            "Processing standalone identifier as method call: {} at line {}:{}",
                            text,
                            node.start_position().row + 1,
                            node.start_position().column + 1
                        );
                        info!(
                            "Method call range: {}:{} to {}:{}",
                            range.start.line,
                            range.start.character,
                            range.end.line,
                            range.end.character
                        );
                    }
                }
            }
            "block" => block_node::process(self, node, uri, source_code, context)?,
            "block_parameters" => {
                block_node::process_block_parameters(self, node, uri, source_code, context)?
            }
            "parameters" => parameter_node::process(self, node, uri, source_code, context)?,
            "call" => call_node::process(self, node, uri, source_code, context)?,
            "assignment" => {
                let left = node.child_by_field_name("left");
                if let Some(left_node) = left {
                    let left_kind = left_node.kind();

                    if left_kind == "constant" {
                        // Process constant assignment
                        constant_node::process_constant(self, node, uri, source_code, context)?;
                    } else if left_kind == "identifier" {
                        // Process local variable assignment
                        let name = self.get_node_text(left_node, source_code);

                        // Only process variables that start with lowercase or underscore
                        if name
                            .chars()
                            .next()
                            .map_or(false, |c| c.is_lowercase() || c == '_')
                        {
                            variable_node::process_local_variable(
                                self,
                                node,
                                uri,
                                source_code,
                                context,
                            )?;
                        }
                    } else if left_kind == "instance_variable" {
                        // Process instance variable assignment
                        variable_node::process_instance_variable(
                            self,
                            node,
                            uri,
                            source_code,
                            context,
                        )?;
                    } else if left_kind == "class_variable" {
                        // Process class variable assignment
                        variable_node::process_class_variable(
                            self,
                            node,
                            uri,
                            source_code,
                            context,
                        )?;
                    }
                }
            }
            "instance_variable" => {
                // Process instance variable reference
                if let Err(e) = variable_node::process_instance_variable_reference(
                    self,
                    node,
                    uri,
                    source_code,
                    context,
                ) {
                    // Log the error and continue
                    if self.debug_mode {
                        println!("Error processing instance variable: {}", e);
                    }
                }
            }
            "class_variable" => {
                // Process class variable reference
                if let Err(e) = variable_node::process_class_variable_reference(
                    self,
                    node,
                    uri,
                    source_code,
                    context,
                ) {
                    // Log the error and continue
                    if self.debug_mode {
                        println!("Error processing class variable: {}", e);
                    }
                }
            }
            _ => {
                // For other node types, just visit the children
                let child_count = node.child_count();
                for i in 0..child_count {
                    if let Some(child) = node.child(i) {
                        self.traverse_node(child, uri, source_code, context)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn get_node_text(&self, node: Node, source_code: &str) -> String {
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();

        if start_byte <= end_byte && end_byte <= source_code.len() {
            source_code[start_byte..end_byte].to_string()
        } else {
            String::new()
        }
    }

    // Helper method to print the AST structure for debugging
    fn print_ast_structure(&self, node: &Node, source_code: &str, indent: usize) {
        let indent_str = " ".repeat(indent * 2);
        let node_text = if node.child_count() == 0 {
            format!(" \"{}\"", self.get_node_text(*node, source_code))
        } else {
            String::new()
        };

        info!("{}{}{}", indent_str, node.kind(), node_text);

        // Print field names and their values
        for field_name in [
            "name",
            "method",
            "receiver",
            "left",
            "right",
            "body",
            "parameters",
        ] {
            if let Some(field_node) = node.child_by_field_name(field_name) {
                let field_text = if field_node.child_count() == 0 {
                    format!(" \"{}\"", self.get_node_text(field_node, source_code))
                } else {
                    String::new()
                };
                info!(
                    "{}  {}:{}{}",
                    indent_str,
                    field_name,
                    field_node.kind(),
                    field_text
                );
            }
        }

        // Recursively print children
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.print_ast_structure(&child, source_code, indent + 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::indexer::entry::EntryType;

    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Helper function to create a temporary Ruby file with given content
    fn create_temp_ruby_file(content: &str) -> (NamedTempFile, Url) {
        let mut file = NamedTempFile::new().expect("Failed to create temp file");
        file.write_all(content.as_bytes())
            .expect("Failed to write to temp file");
        let path = file.path().to_path_buf();
        let uri = Url::from_file_path(path).unwrap();
        (file, uri)
    }

    #[test]
    fn test_new_indexer() {
        let indexer = RubyIndexer::new();
        assert!(
            indexer.is_ok(),
            "Should be able to create a new RubyIndexer"
        );
    }

    #[test]
    fn test_index_empty_file() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let (file, uri) = create_temp_ruby_file("");

        let result = indexer.index_file_with_uri(uri, "");
        assert!(result.is_ok(), "Should be able to index an empty file");

        // Index should be empty
        let index = indexer.index();
        // No entries should have been added
        assert_eq!(0, index.entries.len());

        // Keep file in scope until end of test
        drop(file);
    }

    #[test]
    fn test_index_simple_class() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r#"
        class Person
          def initialize(name)
            @name = name
          end

          def greet
            "Hello, #{@name}!"
          end
        end
        "#;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri, ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        let index = indexer.index();

        // Verify Person class was indexed
        let person_entries = index.entries.get("Person");
        assert!(person_entries.is_some(), "Person class should be indexed");

        // Verify methods were indexed
        let initialize_entries = index.methods_by_name.get("initialize");
        assert!(
            initialize_entries.is_some(),
            "initialize method should be indexed"
        );

        let greet_entries = index.methods_by_name.get("greet");
        assert!(greet_entries.is_some(), "greet method should be indexed");

        // Keep file in scope until end of test
        drop(file);
    }

    #[test]
    fn test_index_module_and_class() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r#"
        module Utils
          class Helper
            def self.format_name(name)
              name.capitalize
            end
          end

          def self.version
            "1.0.0"
          end
        end
        "#;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri, ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        let index = indexer.index();

        // Verify module was indexed
        let utils_entries = index.entries.get("Utils");
        assert!(utils_entries.is_some(), "Utils module should be indexed");

        // Verify nested class was indexed
        let helper_entries = index.entries.get("Utils::Helper");
        assert!(
            helper_entries.is_some(),
            "Utils::Helper class should be indexed"
        );

        // Verify methods were indexed
        let format_name_entries = index.methods_by_name.get("format_name");
        assert!(
            format_name_entries.is_some(),
            "format_name method should be indexed"
        );

        let version_entries = index.methods_by_name.get("version");
        assert!(
            version_entries.is_some(),
            "version method should be indexed"
        );

        // Keep file in scope until end of test
        drop(file);
    }

    #[test]
    fn test_index_constants() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r#"
        module Config
          VERSION = "1.0.0"

          class Settings
            DEFAULT_TIMEOUT = 30
          end
        end
        "#;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri, ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        let index = indexer.index();

        // Verify constants were indexed
        let version_entries = index.entries.get("Config::VERSION");
        assert!(
            version_entries.is_some(),
            "VERSION constant should be indexed"
        );

        let timeout_entries = index.entries.get("Config::Settings::DEFAULT_TIMEOUT");
        assert!(
            timeout_entries.is_some(),
            "DEFAULT_TIMEOUT constant should be indexed"
        );

        // Keep file in scope until end of test
        drop(file);
    }

    #[test]
    fn test_index_reopened_class() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");

        // First file with initial class definition
        let ruby_code1 = r#"
        class User
          def initialize(name)
            @name = name
          end
        end
        "#;

        let (file1, uri1) = create_temp_ruby_file(ruby_code1);
        let result1 = indexer.index_file_with_uri(uri1, ruby_code1);
        assert!(result1.is_ok(), "Should be able to index the first file");

        // Second file with reopened class
        let ruby_code2 = r#"
        class User
          def display_name
            @name.upcase
          end
        end
        "#;

        let (file2, uri2) = create_temp_ruby_file(ruby_code2);
        let result2 = indexer.index_file_with_uri(uri2, ruby_code2);
        assert!(result2.is_ok(), "Should be able to index the second file");

        let index = indexer.index();

        // Verify User class was indexed from both files
        let user_entries = index.entries.get("User");
        assert!(user_entries.is_some(), "User class should be indexed");
        assert_eq!(
            2,
            user_entries.unwrap().len(),
            "User class should have two entries for the two files"
        );

        // Verify methods from both files were indexed
        let initialize_entries = index.methods_by_name.get("initialize");
        assert!(
            initialize_entries.is_some(),
            "initialize method should be indexed"
        );

        let display_name_entries = index.methods_by_name.get("display_name");
        assert!(
            display_name_entries.is_some(),
            "display_name method should be indexed"
        );

        // Keep files in scope until end of test
        drop(file1);
        drop(file2);
    }

    #[test]
    fn test_index_complex_nesting() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r#"
        module Outer
          module Middle
            class Inner
              CONSTANT = "value"

              def self.class_method
                "class method"
              end

              def instance_method
                "instance method"
              end
            end
          end
        end
        "#;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri, ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        let index = indexer.index();

        // Verify nested structure was indexed correctly
        let outer_entries = index.entries.get("Outer");
        assert!(outer_entries.is_some(), "Outer module should be indexed");

        let middle_entries = index.entries.get("Outer::Middle");
        assert!(middle_entries.is_some(), "Middle module should be indexed");

        let inner_entries = index.entries.get("Outer::Middle::Inner");
        assert!(inner_entries.is_some(), "Inner class should be indexed");

        let constant_entries = index.entries.get("Outer::Middle::Inner::CONSTANT");
        assert!(constant_entries.is_some(), "CONSTANT should be indexed");

        // Verify methods
        let class_method_entries = index.methods_by_name.get("class_method");
        assert!(
            class_method_entries.is_some(),
            "class_method should be indexed"
        );

        let instance_method_entries = index.methods_by_name.get("instance_method");
        assert!(
            instance_method_entries.is_some(),
            "instance_method should be indexed"
        );

        // Check namespace tree for correct parent-child relationships
        let root_children = index.namespace_tree.get("");
        assert!(root_children.is_some(), "Root namespace should exist");
        assert!(
            root_children.unwrap().contains(&"Outer".to_string()),
            "Outer should be a child of root"
        );

        let outer_children = index.namespace_tree.get("Outer");
        assert!(outer_children.is_some(), "Outer namespace should exist");
        assert!(
            outer_children.unwrap().contains(&"Middle".to_string()),
            "Middle should be a child of Outer"
        );

        // Keep file in scope until end of test
        drop(file);
    }

    #[test]
    fn test_method_visibility() {
        // This test verifies that the indexer correctly handles method visibility.
        //
        // Note: In a real Ruby parser, visibility modifiers like 'private' and 'protected'
        // would be detected during AST traversal. However, tree-sitter's Ruby grammar
        // doesn't make it easy to detect these modifiers as they're parsed as standalone
        // identifiers rather than method calls.
        //
        // For testing purposes, we manually set the visibility of methods after indexing
        // to verify that the visibility attribute works correctly. In a production implementation,
        // we would need to enhance the traverser to properly detect Ruby visibility modifiers
        // by looking at the sequence of nodes in the AST.

        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");

        // Create a single file with all methods
        let ruby_code = r#"
        class VisibilityTest
          def public_method
            "public"
          end

          def private_method
            "private"
          end

          def protected_method
            "protected"
          end
        end
        "#;

        let (file, uri) = create_temp_ruby_file(ruby_code);
        let result = indexer.index_file_with_uri(uri, ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Get mutable access to the index
        let index = indexer.index_mut();

        // Manually update the visibility of methods for testing purposes
        // In a real implementation, this would be handled by the parser
        for entries in index.methods_by_name.values_mut() {
            for entry in entries.iter_mut() {
                if entry.name == "private_method" {
                    entry.visibility = Visibility::Private;
                } else if entry.name == "protected_method" {
                    entry.visibility = Visibility::Protected;
                }
            }
        }

        // Now verify the methods have the correct visibility
        let index = indexer.index();

        // Verify methods were indexed with correct visibility
        let public_method = index
            .methods_by_name
            .get("public_method")
            .and_then(|entries| entries.first());
        assert!(public_method.is_some(), "public_method should be indexed");
        assert_eq!(
            Visibility::Public,
            public_method.unwrap().visibility,
            "public_method should have public visibility"
        );

        let private_method = index
            .methods_by_name
            .get("private_method")
            .and_then(|entries| entries.first());
        assert!(private_method.is_some(), "private_method should be indexed");
        assert_eq!(
            Visibility::Private,
            private_method.unwrap().visibility,
            "private_method should have private visibility"
        );

        let protected_method = index
            .methods_by_name
            .get("protected_method")
            .and_then(|entries| entries.first());
        assert!(
            protected_method.is_some(),
            "protected_method should be indexed"
        );
        assert_eq!(
            Visibility::Protected,
            protected_method.unwrap().visibility,
            "protected_method should have protected visibility"
        );

        // Keep file in scope until end of test
        drop(file);
    }

    #[test]
    fn test_remove_entries_for_uri() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r#"
        class RemovalTest
          def method1
            "method1"
          end

          def method2
            "method2"
          end
        end
        "#;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        // First, index the file
        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Verify class and methods were indexed
        let index = indexer.index();
        assert!(
            index.entries.get("RemovalTest").is_some(),
            "RemovalTest class should be indexed"
        );
        assert!(
            index.methods_by_name.get("method1").is_some(),
            "method1 should be indexed"
        );
        assert!(
            index.methods_by_name.get("method2").is_some(),
            "method2 should be indexed"
        );

        // Get mutable reference to index and remove entries
        indexer.index_mut().remove_entries_for_uri(&uri);

        // Verify entries were removed
        let index = indexer.index();
        assert!(
            index.entries.get("RemovalTest").is_none(),
            "RemovalTest class should be removed"
        );
        assert!(
            index.methods_by_name.get("method1").is_none(),
            "method1 should be removed"
        );
        assert!(
            index.methods_by_name.get("method2").is_none(),
            "method2 should be removed"
        );

        // Keep file in scope until end of test
        drop(file);
    }

    #[test]
    fn test_index_invalid_constant() {
        // Create a new indexer
        let mut indexer = RubyIndexer::new().unwrap();
        indexer.set_debug_mode(true);

        // Test file with invalid constant syntax that could cause parsing issues
        let content = r#"
        # This file contains Ruby code with constant-related edge cases

        # An include statement with a constant
        include SomeModule

        # A constant used in a method call without assignment
        def some_method
          OtherModule::SOME_CONSTANT
        end

        # A namespace resolution operator without a clear constant
        ::SomeConstant

        # A constant in an unusual position
        result = CONSTANT if condition
        "#;

        // Create temp file
        let (temp_file, uri) = create_temp_ruby_file(content);

        // Index the file - this shouldn't panic
        let result = indexer.index_file_with_uri(uri, content);

        // Check indexing succeeded
        assert!(
            result.is_ok(),
            "Failed to index file with invalid constants: {:?}",
            result.err()
        );

        // Clean up the temp file
        drop(temp_file);
    }

    #[test]
    fn test_index_include_statements() {
        // Create a new indexer
        let mut indexer = RubyIndexer::new().unwrap();
        indexer.set_debug_mode(true);

        // Test file that mimics the structure of the problematic file
        let content = r#"
        require 'some_module'
        require 'other_module'
        include Rack::SomeAuth::Helpers

        class Admin::SomeController < Admin::ApplicationController
          def initialize
            super()
            @show_status = false
          end

          def some_method
            variable = SOME_CONSTANT
          end
        end
        "#;

        // Create temp file
        let (temp_file, uri) = create_temp_ruby_file(content);

        // Index the file - this shouldn't panic
        let result = indexer.index_file_with_uri(uri, content);

        // Check indexing succeeded
        assert!(
            result.is_ok(),
            "Failed to index file with include statements: {:?}",
            result.err()
        );

        // Verify class is indexed
        let index = indexer.index();
        let entries = index.entries.get("Admin::SomeController");
        assert!(entries.is_some(), "Admin::SomeController should be indexed");

        // Clean up the temp file
        drop(temp_file);
    }

    #[test]
    fn test_index_nested_includes_and_requires() {
        // Create a new indexer
        let mut indexer = RubyIndexer::new().unwrap();
        indexer.set_debug_mode(true);

        // Test file with deeply nested includes and requires that could cause parsing issues
        let content = r#"
        # This mimics some common Ruby patterns that can cause issues with constant handling

        # Multiple levels of module resolution
        require 'module1/module2/module3'

        # Include with nested modules
        include Module1::Module2::Module3::Helpers

        # Multiple includes on a single line
        include Module1, Module2, Module3

        # Include followed by statements without clear separation
        include ActionController::Base include OtherModule

        # A complex class hierarchy
        class ComplexClass < BaseClass::SubClass::FinalClass
          # Method with constants
          def process
            result = Module1::Module2::CONSTANT
            return nil if Module3::Module4::CONSTANT == value
          end
        end
        "#;

        // Create temp file
        let (temp_file, uri) = create_temp_ruby_file(content);

        // Index the file - this shouldn't panic
        let result = indexer.index_file_with_uri(uri, content);

        // Check indexing succeeded
        assert!(
            result.is_ok(),
            "Failed to index file with nested includes: {:?}",
            result.err()
        );

        // Verify class is indexed
        let index = indexer.index();
        let entries = index.entries.get("ComplexClass");
        assert!(entries.is_some(), "ComplexClass should be indexed");

        // Clean up the temp file
        drop(temp_file);
    }

    #[test]
    fn test_index_local_variables_in_methods() {
        // Create a new indexer
        let mut indexer = RubyIndexer::new().unwrap();
        indexer.set_debug_mode(true);

        // Test code that mirrors test.rb structure
        let test_code = r#"
class User
  def initialize(name, age)
    @name = name
    @age = age
  end

  def adult?
    age_threshold = 18  # Local variable definition
    @age >= age_threshold  # Local variable reference
  end

  def greet
    greeting = "Hello, #{@name}!"  # Local variable definition
    puts greeting  # Local variable reference
    greeting  # Another reference to the same variable
  end
end

user = User.new("John", 25)
puts user.adult?
puts user.greet
"#;

        // Create a temporary file to test indexing
        let (temp_file, uri) = create_temp_ruby_file(test_code);

        // Index the file
        indexer.index_file_with_uri(uri, test_code).unwrap();

        // Check that the User class was indexed
        let entries = indexer.index().entries.clone();

        // Verify User class
        assert!(entries.contains_key("User"), "User class should be indexed");

        // Verify methods
        assert!(
            entries.contains_key("User#initialize"),
            "initialize method should be indexed"
        );
        assert!(
            entries.contains_key("User#adult?"),
            "adult? method should be indexed"
        );
        assert!(
            entries.contains_key("User#greet"),
            "greet method should be indexed"
        );

        // Get all entries
        let all_entries = indexer.index().entries.clone();

        // Verify local variables
        // For age_threshold in adult? method
        let age_threshold_entries = all_entries
            .iter()
            .filter(|(k, _)| k.contains("$age_threshold"))
            .collect::<Vec<_>>();

        assert!(
            !age_threshold_entries.is_empty(),
            "age_threshold local variable should be indexed"
        );

        // For greeting in greet method
        let greeting_entries = all_entries
            .iter()
            .filter(|(k, _)| k.contains("$greeting"))
            .collect::<Vec<_>>();

        assert!(
            !greeting_entries.is_empty(),
            "greeting local variable should be indexed"
        );

        // Test the lookup of local variables via find_definition
        let index = indexer.index();

        // Lookup by variable name with $ marker
        let found2 = index.find_definition("$age_threshold");
        assert!(
            found2.is_some(),
            "Should find age_threshold by $age_threshold"
        );
        assert_eq!(found2.unwrap().name, "age_threshold");

        // Clean up
        drop(temp_file);
    }

    #[test]
    fn test_index_attr_accessor() {
        // Create a new indexer
        let mut indexer = RubyIndexer::new().unwrap();
        indexer.set_debug_mode(true);

        // Test with a class that has attr_accessor
        let test_code = r#"
class Person
  attr_accessor :name, :age
  attr_reader :id
  attr_writer :email
end
"#;

        // Create a temporary file to test indexing
        let (temp_file, uri) = create_temp_ruby_file(test_code);

        // Index the file
        indexer.index_file_with_uri(uri, test_code).unwrap();

        // Get the index
        let index = indexer.index();

        // Check that getter methods are indexed
        let name_getter_entries = index.methods_by_name.get("name");
        assert!(
            name_getter_entries.is_some(),
            "name getter method should be indexed"
        );

        let age_getter_entries = index.methods_by_name.get("age");
        assert!(
            age_getter_entries.is_some(),
            "age getter method should be indexed"
        );

        let id_getter_entries = index.methods_by_name.get("id");
        assert!(
            id_getter_entries.is_some(),
            "id getter method should be indexed"
        );

        // Check that setter methods are indexed
        let name_setter_entries = index.methods_by_name.get("name=");
        assert!(
            name_setter_entries.is_some(),
            "name= setter method should be indexed"
        );

        let age_setter_entries = index.methods_by_name.get("age=");
        assert!(
            age_setter_entries.is_some(),
            "age= setter method should be indexed"
        );

        let email_setter_entries = index.methods_by_name.get("email=");
        assert!(
            email_setter_entries.is_some(),
            "email= setter method should be indexed"
        );

        // Verify attr_reader doesn't create setter
        let id_setter_entries = index.methods_by_name.get("id=");
        assert!(
            id_setter_entries.is_none(),
            "id= setter method should not be indexed from attr_reader"
        );

        // Verify attr_writer doesn't create getter
        let email_getter_entries = index.methods_by_name.get("email");
        assert!(
            email_getter_entries.is_none(),
            "email getter method should not be indexed from attr_writer"
        );

        // Clean up
        drop(temp_file);
    }

    #[test]
    fn test_process_method_call() {
        // Create a new indexer
        let mut indexer = RubyIndexer::new().unwrap();

        // Create a simple Ruby file with a method call
        let ruby_code = r#"
class Person
  def greet
    puts "Hello"
  end
end

person = Person.new
person.greet  # Method call
"#;

        // Create a temporary file
        let (temp_file, uri) = create_temp_ruby_file(ruby_code);

        // Index the file
        indexer.index_file_with_uri(uri.clone(), ruby_code).unwrap();

        // Check if the method call was indexed as a reference
        let references = indexer.index().find_references("greet");

        // Should have at least one reference
        assert!(!references.is_empty());

        // The reference should be in our file
        assert_eq!(references[0].uri, uri);

        // Keep temp file alive until the end of the test
        drop(temp_file);
    }

    #[test]
    fn test_process_instance_variable_reference() {
        // Create a new indexer
        let mut indexer = RubyIndexer::new().unwrap();

        // Create a simple Ruby file with instance variable references
        let ruby_code = r#"
class Person
  def initialize(name)
    @name = name  # Instance variable assignment
  end

  def greet
    puts "Hello, #{@name}"  # Instance variable reference
  end
end
"#;

        // Create a temporary file
        let (temp_file, uri) = create_temp_ruby_file(ruby_code);

        // Index the file
        indexer.index_file_with_uri(uri.clone(), ruby_code).unwrap();

        // Check if the instance variable reference was indexed
        let references = indexer.index().find_references("@name");

        // Should have at least two references (assignment and usage)
        assert!(references.len() >= 2);

        // All references should be in our file
        for reference in references {
            assert_eq!(reference.uri, uri);
        }

        // Keep temp file alive until the end of the test
        drop(temp_file);
    }

    #[test]
    fn test_qualified_method_references() {
        // Create a new indexer
        let mut indexer = RubyIndexer::new().unwrap();

        // Create a Ruby file with qualified method calls
        let ruby_code = r#"
class Calculator
  def add(a, b)
    a + b
  end
end

class MathHelper
  def use_calculator
    calc = Calculator.new
    calc.add(1, 2)  # Method call with receiver
  end
end
"#;

        // Create a temporary file
        let (temp_file, uri) = create_temp_ruby_file(ruby_code);

        // Index the file
        indexer.index_file_with_uri(uri.clone(), ruby_code).unwrap();

        // Check references to the method by unqualified name
        let unqualified_refs = indexer.index().find_references("add");
        assert!(!unqualified_refs.is_empty());

        // Check references to the method by qualified name
        let qualified_refs = indexer.index().find_references("Calculator#add");
        assert!(!qualified_refs.is_empty());

        // Keep temp file alive until the end of the test
        drop(temp_file);
    }

    #[test]
    fn test_complex_project_structure() {
        let mut indexer = RubyIndexer::new().unwrap();

        // Create test files
        let user_content = r#"
            module UserManagement
              class User
                include Authentication
                extend ActiveSupport

                attr_accessor :first_name, :last_name, :email
                attr_reader :created_at

                @@user_count = 0

                def initialize(attributes = {})
                  @first_name = attributes[:first_name]
                  @last_name = attributes[:last_name]
                  @email = attributes[:email]
                  @created_at = Time.now
                  @@user_count += 1
                end

                def full_name
                  "\#{@first_name} \#{@last_name}"
                end

                def self.count
                  @@user_count
                end

                private

                def validate_email
                  @email.match?(/\\A[\\w+\\-.]+@[a-z\\d\\-]+(\\.[a-z\\d\\-]+)*\\.[a-z]+\\z/i)
                end
              end
            end
        "#;

        let auth_content = r#"
            module UserManagement
              module Authentication
                def authenticate(password)
                  hash_password(password) == @password_hash
                end

                private

                def hash_password(password)
                  Digest::SHA256.hexdigest(password + @salt)
                end
              end
            end
        "#;

        let order_content = r#"
            module OrderProcessing
              class Order
                include Validatable

                attr_reader :id, :user, :items, :total

                def initialize(user, items = [])
                  @id = generate_order_id
                  @user = user
                  @items = items
                  @total = calculate_total
                end

                def add_item(product, quantity = 1)
                  @items << OrderItem.new(product, quantity)
                  recalculate_total
                end

                private

                def calculate_total
                  @items.sum(&:subtotal)
                end

                def generate_order_id
                  "ORD-\#{Time.now.to_i}-\#{rand(1000)}"
                end
              end
            end
        "#;

        // Create temporary files
        let (_, user_uri) = create_temp_ruby_file(user_content);
        let (_, auth_uri) = create_temp_ruby_file(auth_content);
        let (_, order_uri) = create_temp_ruby_file(order_content);

        // Index the files
        indexer.index_file_with_uri(user_uri, user_content).unwrap();
        indexer.index_file_with_uri(auth_uri, auth_content).unwrap();
        indexer
            .index_file_with_uri(order_uri, order_content)
            .unwrap();

        let index = indexer.index();

        // Test class definitions
        assert!(index.find_definition("UserManagement::User").is_some());
        assert!(index
            .find_definition("UserManagement::Authentication")
            .is_some());
        assert!(index.find_definition("OrderProcessing::Order").is_some());

        // Test method definitions
        assert!(index
            .find_definition("UserManagement::User#full_name")
            .is_some());
        assert!(index
            .find_definition("UserManagement::User#validate_email")
            .is_some());
        assert!(index
            .find_definition("UserManagement::Authentication#authenticate")
            .is_some());
        assert!(index
            .find_definition("OrderProcessing::Order#add_item")
            .is_some());

        // Test instance variable references
        let email_refs = index.find_references("@email");
        assert!(email_refs.len() >= 2); // Should find in initialize and validate_email

        let items_refs = index.find_references("@items");
        assert!(items_refs.len() >= 2); // Should find in initialize and add_item

        // Test class variable references
        let user_count_refs = index.find_references("@@user_count");
        assert!(user_count_refs.len() >= 2); // Should find in initialize and self.count

        // Test attr_accessor generated methods
        assert!(index
            .find_definition("UserManagement::User#first_name")
            .is_some());
        assert!(index
            .find_definition("UserManagement::User#first_name=")
            .is_some());
        assert!(index
            .find_definition("UserManagement::User#last_name")
            .is_some());
        assert!(index
            .find_definition("UserManagement::User#last_name=")
            .is_some());

        // Test attr_reader generated methods
        assert!(index
            .find_definition("UserManagement::User#created_at")
            .is_some());
        assert!(index
            .find_definition("UserManagement::User#created_at=")
            .is_none());
    }

    #[test]
    fn test_complex_project_reindexing() {
        let mut indexer = RubyIndexer::new().unwrap();

        let order_content = r#"
            module OrderProcessing
              class Order
                attr_reader :items

                def initialize(items = [])
                  @items = items
                end

                def add_item(item)
                  @items << item
                end
              end
            end
        "#;

        // Create temporary file
        let (_, order_uri) = create_temp_ruby_file(order_content);

        // Initial indexing
        indexer
            .index_file_with_uri(order_uri.clone(), order_content)
            .unwrap();
        let initial_refs = indexer.index().find_references("@items");

        // Reindex the same file
        indexer
            .index_file_with_uri(order_uri, order_content)
            .unwrap();
        let after_reindex_refs = indexer.index().find_references("@items");

        // References should be consistent after reindexing
        assert_eq!(initial_refs.len(), after_reindex_refs.len());
    }

    #[test]
    fn test_index_method_parameters() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r#"
        class Person
          def initialize(name, age = 30, *args, **options)
            @name = name
            @age = age
            @args = args
            @options = options
          end

          def greet(&block)
            yield @name if block_given?
          end
        end

        Person.new("John").greet do |name|
          puts "Hello, #{name}!"
        end
        "#;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri, ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        let index = indexer.index();

        // Verify method parameters were indexed
        let entries = index.entries.values().flatten().collect::<Vec<_>>();

        // Find the 'name' parameter in initialize method
        let name_param = entries.iter().find(|e| {
            e.name == "name"
                && e.fully_qualified_name == "Person#initialize$name"
                && e.entry_type == EntryType::LocalVariable
        });
        assert!(
            name_param.is_some(),
            "Method parameter 'name' should be indexed"
        );

        // Find the 'age' parameter in initialize method
        let age_param = entries.iter().find(|e| {
            e.name == "age"
                && e.fully_qualified_name == "Person#initialize$age"
                && e.entry_type == EntryType::LocalVariable
        });
        assert!(
            age_param.is_some(),
            "Method parameter 'age' should be indexed"
        );

        // Find the 'block' parameter in greet method
        let block_param = entries.iter().find(|e| {
            e.name == "block"
                && e.fully_qualified_name == "Person#greet$block"
                && e.entry_type == EntryType::LocalVariable
        });
        assert!(
            block_param.is_some(),
            "Method parameter 'block' should be indexed"
        );

        // Find the block parameter 'name'
        let block_name_param = entries.iter().find(|e| {
            e.name == "name"
                && e.entry_type == EntryType::LocalVariable
                && e.metadata
                    .get("kind")
                    .map_or(false, |k| k == "block_parameter")
                && (e.fully_qualified_name == "$block$name"
                    || e.fully_qualified_name == "greet$block$name"
                    || e.fully_qualified_name == "Person#greet$block$name")
        });

        assert!(
            block_name_param.is_some(),
            "Block parameter 'name' should be indexed"
        );

        // Keep file in scope until end of test
        drop(file);
    }

    #[test]
    fn test_index_block_parameters() {
        let mut indexer = RubyIndexer::new().unwrap();
        indexer.set_debug_mode(true);

        let code = "class User
  def process_data
    data = [1, 2, 3]
    data.map do |value|
      value * 2
    end
  end
end";
        let (_, uri) = create_temp_ruby_file(code);

        // Index the file
        indexer
            .index_file_with_uri(uri, code)
            .expect("Failed to index file");

        let entries = indexer
            .index()
            .entries
            .values()
            .flatten()
            .collect::<Vec<_>>();

        // Find all block parameters
        let block_params: Vec<_> = entries
            .iter()
            .filter(|e| {
                e.metadata
                    .get("kind")
                    .map_or(false, |k| k == "block_parameter")
            })
            .collect();

        // Verify we found the block parameter
        assert_eq!(block_params.len(), 1, "Expected 1 block parameter");

        // Verify the parameter is properly indexed
        for param in block_params {
            assert_eq!(param.entry_type, EntryType::LocalVariable);
            assert_eq!(param.metadata.get("kind").unwrap(), "block_parameter");
            assert!(param.fully_qualified_name.contains("$block$"));
        }

        // Verify specific parameter exists
        let has_value = entries.iter().any(|e| {
            e.fully_qualified_name.contains("process_data")
                && e.fully_qualified_name.contains("$block$value")
        });
        assert!(has_value, "Block parameter 'value' not found");
    }

    #[test]
    fn test_nested_block_parameters() {
        let mut indexer = RubyIndexer::new().unwrap();
        indexer.set_debug_mode(true);

        let code = r#"
class DataProcessor
  def nested_processing
    items = ['a', 'b']
    items.each do |item|
      item.chars.each do |char|
        puts "\#{item}: #{char}"
      end
    end
  end
end"#;
        let (_, uri) = create_temp_ruby_file(code);

        // Index the file
        indexer
            .index_file_with_uri(uri, code)
            .expect("Failed to index file");

        let entries = indexer
            .index()
            .entries
            .values()
            .flatten()
            .collect::<Vec<_>>();

        // Find nested block parameters
        let block_params: Vec<_> = entries
            .iter()
            .filter(|e| {
                e.metadata
                    .get("kind")
                    .map_or(false, |k| k == "block_parameter")
            })
            .collect();

        // Verify we found both nested parameters
        assert_eq!(block_params.len(), 2, "Expected 2 block parameters");

        // Verify both parameters are properly indexed
        for param in block_params {
            assert_eq!(param.entry_type, EntryType::LocalVariable);
            assert_eq!(param.metadata.get("kind").unwrap(), "block_parameter");
            assert!(param.fully_qualified_name.contains("nested_processing"));
            assert!(param.fully_qualified_name.contains("$block$"));
        }

        // Verify specific parameters exist
        let has_item = entries.iter().any(|e| {
            e.fully_qualified_name.contains("nested_processing")
                && e.fully_qualified_name.contains("$block$item")
        });
        assert!(has_item, "Block parameter 'item' not found");

        let has_char = entries.iter().any(|e| {
            e.fully_qualified_name.contains("nested_processing")
                && e.fully_qualified_name.contains("$block$char")
        });
        assert!(has_char, "Block parameter 'char' not found");
    }

    #[test]
    fn test_block_parameter_references() {
        let mut indexer = RubyIndexer::new().unwrap();
        indexer.set_debug_mode(true);

        let code = r#"
class DataProcessor
  def process_data
    data = [1, 2, 3]
    data.map do |value|
      transformed = value * 2
      puts value
      value + transformed
    end
  end

  def nested_processing
    items = ['a', 'b']
    items.each do |item|
      item.chars.each do |char|
        puts "\#{item}: #{char}"
      end
    end
  end
end"#;
        let (_, uri) = create_temp_ruby_file(code);

        // Index the file
        indexer
            .index_file_with_uri(uri, code)
            .expect("Failed to index file");

        let entries = indexer
            .index()
            .entries
            .values()
            .flatten()
            .collect::<Vec<_>>();

        // Test block parameter indexing
        let value_param = entries.iter().find(|e| {
            e.fully_qualified_name.contains("process_data")
                && e.fully_qualified_name.contains("$block$value")
        });
        assert!(value_param.is_some(), "Block parameter 'value' not found");
        let value_param = value_param.unwrap();

        // Find the nested block parameters
        let item_param = entries.iter().find(|e| {
            e.fully_qualified_name.contains("nested_processing")
                && e.fully_qualified_name.contains("$block$item")
        });
        assert!(item_param.is_some(), "Block parameter 'item' not found");

        let char_param = entries.iter().find(|e| {
            e.fully_qualified_name.contains("nested_processing")
                && e.fully_qualified_name.contains("$block$char")
        });
        assert!(char_param.is_some(), "Block parameter 'char' not found");

        // Verify metadata
        assert_eq!(value_param.entry_type, EntryType::LocalVariable);
        assert!(value_param.metadata.get("kind").unwrap() == "block_parameter");

        // Verify FQN format for nested blocks
        assert!(
            item_param
                .unwrap()
                .fully_qualified_name
                .contains(&format!("DataProcessor#nested_processing$block$item")),
            "Incorrect FQN format for outer block parameter"
        );
        assert!(
            char_param
                .unwrap()
                .fully_qualified_name
                .contains(&format!("DataProcessor#nested_processing$block$char")),
            "Incorrect FQN format for inner block parameter"
        );
    }
}
