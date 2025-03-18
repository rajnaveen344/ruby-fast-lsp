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
use utils::{get_indexer_node_text, node_to_range};

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
                let text = get_indexer_node_text(self, node, source_code);

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
                        let name = get_indexer_node_text(self, left_node, source_code);

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

    pub fn get_node_text(&self, node: Node, source_code: &str) -> String {
        utils::get_node_text(node, source_code)
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
}
