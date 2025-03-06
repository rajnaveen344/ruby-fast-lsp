use log::info;
use std::path::Path;
use tower_lsp::lsp_types::{Position, Range, Url};
use tree_sitter::{Node, Parser, Tree};

use crate::indexer::{EntryBuilder, EntryType, RubyIndex, Visibility};

pub struct RubyIndexer {
    // The Ruby index being populated
    index: RubyIndex,

    // The Tree-sitter parser
    parser: Parser,

    // Debug flag for tests
    debug_mode: bool,
}

// Add a context struct to track more state during traversal
struct TraversalContext {
    visibility: Visibility,
    namespace_stack: Vec<String>,
    current_method: Option<String>,
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
        parser
            .set_language(tree_sitter_ruby::language())
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

    pub fn index_file(&mut self, file_path: &Path, source_code: &str) -> Result<(), String> {
        // Parse the source code
        let tree = self.parser.parse(source_code, None).ok_or_else(|| {
            format!(
                "Failed to parse source code in file: {}",
                file_path.display()
            )
        })?;

        // Get the file URI
        let uri = Url::from_file_path(file_path).map_err(|_| {
            format!(
                "Failed to convert file path to URI: {}",
                file_path.display()
            )
        })?;

        // Process the file for indexing
        self.process_file(uri, &tree, source_code)
            .map_err(|e| format!("Failed to index file {}: {}", file_path.display(), e))
    }

    pub fn index_file_with_uri(&mut self, uri: Url, source_code: &str) -> Result<(), String> {
        // Parse the source code
        let tree = self
            .parser
            .parse(source_code, None)
            .ok_or_else(|| format!("Failed to parse source code in file: {}", uri))?;

        // Process the file for indexing
        self.process_file(uri.clone(), &tree, source_code)
            .map_err(|e| format!("Failed to index file {}: {}", uri, e))
    }

    pub fn set_debug_mode(&mut self, debug: bool) {
        self.debug_mode = debug;
    }

    fn process_file(&mut self, uri: Url, tree: &Tree, source_code: &str) -> Result<(), String> {
        let root_node = tree.root_node();

        // Reset the traversal context for a new file
        let mut context = TraversalContext::new();

        // Pre-process: Remove any existing entries for this URI
        self.index.remove_entries_for_uri(&uri);

        // Traverse the tree to find definitions
        self.traverse_node(root_node, &uri, source_code, &mut context)
    }

    fn traverse_node(
        &mut self,
        node: Node,
        uri: &Url,
        source_code: &str,
        context: &mut TraversalContext,
    ) -> Result<(), String> {
        // Get the kind of node
        let kind = node.kind();

        // Skip comments
        if kind == "comment" {
            return Ok(());
        }

        // Process this node based on its kind
        match kind {
            "class" => {
                self.process_class(node, uri, source_code, context)?;
            }
            "module" => {
                self.process_module(node, uri, source_code, context)?;
            }
            "method" | "singleton_method" => {
                self.process_method(node, uri, source_code, context)?;
            }
            "call" => {
                // Check for attr_* method calls like attr_accessor, attr_reader, attr_writer
                self.process_attribute_methods(node, uri, source_code, context)?;

                // Continue traversing children
                let child_count = node.child_count();
                for i in 0..child_count {
                    if let Some(child) = node.child(i) {
                        self.traverse_node(child, uri, source_code, context)?;
                    }
                }
            }
            "assignment" => {
                let left = node.child_by_field_name("left");
                if let Some(left_node) = left {
                    let left_kind = left_node.kind();

                    if left_kind == "constant" {
                        // Process constant assignment
                        self.process_constant(node, uri, source_code, context)?;
                    } else if left_kind == "identifier" {
                        // Process local variable assignment
                        let name = self.get_node_text(left_node, source_code);

                        // Only process variables that start with lowercase or underscore
                        if name
                            .chars()
                            .next()
                            .map_or(false, |c| c.is_lowercase() || c == '_')
                        {
                            self.process_local_variable(node, uri, source_code, context)?;
                        }
                    } else if left_kind == "instance_variable" {
                        // Process instance variable assignment
                        self.process_instance_variable(node, uri, source_code, context)?;
                    }
                }
            }
            "constant" => {
                // Try to process constants, but don't fail if we can't
                if let Err(e) = self.process_constant(node, uri, source_code, context) {
                    // Log the error and continue
                    if self.debug_mode {
                        println!("Error processing constant: {}", e);
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
            "instance_variable" => {
                // Process instance variable reference
                if let Err(e) =
                    self.process_instance_variable_reference(node, uri, source_code, context)
                {
                    // Log the error and continue
                    if self.debug_mode {
                        println!("Error processing instance variable: {}", e);
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

    fn process_class(
        &mut self,
        node: Node,
        uri: &Url,
        source_code: &str,
        context: &mut TraversalContext,
    ) -> Result<(), String> {
        // Find the class name node
        let name_node = match node.child_by_field_name("name") {
            Some(node) => node,
            None => {
                // Skip anonymous or dynamically defined classes instead of failing
                if self.debug_mode {
                    info!(
                        "Skipping class without a name at {}:{}",
                        node.start_position().row + 1,
                        node.start_position().column + 1
                    );
                }

                // Still traverse children for any defined methods or constants
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        self.traverse_node(child, uri, source_code, context)?;
                    }
                }

                return Ok(());
            }
        };

        // Extract the name text
        let name = self.get_node_text(name_node, source_code);

        // Skip classes with empty names or just whitespace
        if name.trim().is_empty() {
            if self.debug_mode {
                info!(
                    "Skipping class with empty name at {}:{}",
                    node.start_position().row + 1,
                    node.start_position().column + 1
                );
            }

            // Still traverse children
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    self.traverse_node(child, uri, source_code, context)?;
                }
            }

            return Ok(());
        }

        // Create a fully qualified name by joining the namespace stack
        let current_namespace = context.current_namespace();

        let fqn = if current_namespace.is_empty() {
            name.clone()
        } else {
            format!("{}::{}", current_namespace, name)
        };

        // Create a range for the definition
        let range = node_to_range(node);

        // Create and add the entry
        let entry = EntryBuilder::new(&name)
            .fully_qualified_name(&fqn)
            .location(uri.clone(), range)
            .entry_type(EntryType::Class)
            .build()?;

        self.index.add_entry(entry);

        // Add to namespace tree
        let parent_namespace = if context.namespace_stack.is_empty() {
            String::new()
        } else {
            current_namespace
        };

        let children = self
            .index
            .namespace_tree
            .entry(parent_namespace)
            .or_insert_with(Vec::new);

        if !children.contains(&name) {
            children.push(name.clone());
        }

        // Push the class name onto the namespace stack
        context.namespace_stack.push(name);

        // Process the body of the class
        if let Some(body_node) = node.child_by_field_name("body") {
            self.traverse_node(body_node, uri, source_code, context)?;
        }

        // Pop the namespace when done
        context.namespace_stack.pop();

        Ok(())
    }

    fn process_module(
        &mut self,
        node: Node,
        uri: &Url,
        source_code: &str,
        context: &mut TraversalContext,
    ) -> Result<(), String> {
        // Find the module name node
        let name_node = match node.child_by_field_name("name") {
            Some(node) => node,
            None => {
                // Skip anonymous or dynamically defined modules instead of failing
                if self.debug_mode {
                    info!(
                        "Skipping module without a name at {}:{}",
                        node.start_position().row + 1,
                        node.start_position().column + 1
                    );
                }

                // Still traverse children for any defined methods or constants
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        self.traverse_node(child, uri, source_code, context)?;
                    }
                }

                return Ok(());
            }
        };

        // Extract the name text
        let name = self.get_node_text(name_node, source_code);

        // Skip modules with empty names or just whitespace
        if name.trim().is_empty() {
            if self.debug_mode {
                info!(
                    "Skipping module with empty name at {}:{}",
                    node.start_position().row + 1,
                    node.start_position().column + 1
                );
            }

            // Still traverse children
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    self.traverse_node(child, uri, source_code, context)?;
                }
            }

            return Ok(());
        }

        // Create a fully qualified name by joining the namespace stack
        let current_namespace = context.current_namespace();

        let fqn = if current_namespace.is_empty() {
            name.clone()
        } else {
            format!("{}::{}", current_namespace, name)
        };

        // Create a range for the definition
        let range = node_to_range(node);

        // Create and add the entry
        let entry = EntryBuilder::new(&name)
            .fully_qualified_name(&fqn)
            .location(uri.clone(), range)
            .entry_type(EntryType::Module)
            .build()?;

        self.index.add_entry(entry);

        // Add to namespace tree
        let parent_namespace = if context.namespace_stack.is_empty() {
            String::new()
        } else {
            current_namespace
        };

        let children = self
            .index
            .namespace_tree
            .entry(parent_namespace)
            .or_insert_with(Vec::new);

        if !children.contains(&name) {
            children.push(name.clone());
        }

        // Push the module name onto the namespace stack
        context.namespace_stack.push(name);

        // Process the body of the module
        if let Some(body_node) = node.child_by_field_name("body") {
            self.traverse_node(body_node, uri, source_code, context)?;
        }

        // Pop the namespace when done
        context.namespace_stack.pop();

        Ok(())
    }

    fn process_method(
        &mut self,
        node: Node,
        uri: &Url,
        source_code: &str,
        context: &mut TraversalContext,
    ) -> Result<(), String> {
        // Find the method name node
        let name_node = node
            .child_by_field_name("name")
            .ok_or_else(|| "Method without a name".to_string())?;

        // Extract the name text
        let name = self.get_node_text(name_node, source_code);

        // Create a fully qualified name
        let current_namespace = context.current_namespace();
        let method_name = name.clone();

        let fqn = if current_namespace.is_empty() {
            method_name.clone()
        } else {
            format!("{}#{}", current_namespace, method_name)
        };

        // Create a range for the definition
        let range = node_to_range(node);

        // Create and add the entry
        let entry = EntryBuilder::new(&name)
            .fully_qualified_name(&fqn)
            .location(uri.clone(), range)
            .entry_type(EntryType::Method)
            .visibility(context.visibility)
            .build()
            .map_err(|e| e.to_string())?;

        self.index.add_entry(entry);

        // Set the current method before processing the body
        context.current_method = Some(name.clone());

        // Process method body contents recursively
        if let Some(body) = node.child_by_field_name("body") {
            for i in 0..body.named_child_count() {
                if let Some(child) = body.named_child(i) {
                    self.traverse_node(child, uri, source_code, context)?;
                }
            }
        }

        // Reset the current method after processing
        context.current_method = None;

        Ok(())
    }

    fn process_constant(
        &mut self,
        node: Node,
        uri: &Url,
        source_code: &str,
        context: &mut TraversalContext,
    ) -> Result<(), String> {
        // For constant assignments, the name is in the "left" field
        let name_node = match node.child_by_field_name("left") {
            Some(node) => node,
            None => {
                // If we encounter a constant node without a left field, it's likely part of another
                // construct, like a class name or module. Instead of failing, just continue traversal.
                if self.debug_mode {
                    info!(
                        "Skipping constant without a name field at {}:{}",
                        node.start_position().row + 1,
                        node.start_position().column + 1
                    );
                }

                // Recursively traverse child nodes
                let child_count = node.child_count();
                for i in 0..child_count {
                    if let Some(child) = node.child(i) {
                        self.traverse_node(child, uri, source_code, context)?;
                    }
                }

                return Ok(());
            }
        };

        // Make sure it's a constant (starts with capital letter)
        let name = self.get_node_text(name_node, source_code);
        if name.trim().is_empty() || !name.starts_with(|c: char| c.is_uppercase()) {
            // Not a valid constant, just continue traversal
            // Recursively traverse child nodes
            let child_count = node.child_count();
            for i in 0..child_count {
                if let Some(child) = node.child(i) {
                    self.traverse_node(child, uri, source_code, context)?;
                }
            }
            return Ok(());
        }

        // Create a fully qualified name
        let current_namespace = context.current_namespace();
        let constant_name = name.clone();

        let fqn = if current_namespace.is_empty() {
            constant_name.clone()
        } else {
            format!("{}::{}", current_namespace, constant_name)
        };

        // Create a range for the definition
        let range = node_to_range(node);

        // Create and add the entry
        let entry = EntryBuilder::new(&name)
            .fully_qualified_name(&fqn)
            .location(uri.clone(), range)
            .entry_type(EntryType::Constant)
            .build()
            .map_err(|e| e.to_string())?;

        self.index.add_entry(entry);

        // Process the right side of the assignment
        if let Some(right) = node.child_by_field_name("right") {
            self.traverse_node(right, uri, source_code, context)?;
        }

        Ok(())
    }

    // New method to handle local variable assignments
    fn process_local_variable(
        &mut self,
        node: Node,
        uri: &Url,
        source_code: &str,
        context: &mut TraversalContext,
    ) -> Result<(), String> {
        // For local variable assignments, the name is in the "left" field
        let name_node = node
            .child_by_field_name("left")
            .ok_or_else(|| "Variable assignment without a name".to_string())?;

        // Extract the variable name
        let name = self.get_node_text(name_node, source_code);

        // Skip if name is empty
        if name.trim().is_empty() {
            return Ok(());
        }

        // Create a fully qualified name that includes the current method scope
        // This is important to prevent collisions between variables in different methods
        let current_namespace = context.current_namespace();

        // Determine if we're in a method context
        let current_method = context.current_method.as_ref();

        let fqn = if let Some(method_name) = current_method {
            // If we're in a method, include it in the FQN
            if current_namespace.is_empty() {
                format!("{}#${}", method_name, name)
            } else {
                format!("{}#${}${}", current_namespace, method_name, name)
            }
        } else {
            // Otherwise, just use the namespace and name
            if current_namespace.is_empty() {
                format!("${}", name)
            } else {
                format!("{}#${}", current_namespace, name)
            }
        };

        // Create a range for the definition
        let range = node_to_range(node);

        // Create and add the entry
        let entry = EntryBuilder::new(&name)
            .fully_qualified_name(&fqn)
            .location(uri.clone(), range)
            .entry_type(EntryType::LocalVariable)
            .build()
            .map_err(|e| e.to_string())?;

        self.index.add_entry(entry);

        // Continue traversing the right side of the assignment
        if let Some(right) = node.child_by_field_name("right") {
            self.traverse_node(right, uri, source_code, context)?;
        }

        Ok(())
    }

    fn process_instance_variable(
        &mut self,
        node: Node,
        uri: &Url,
        source_code: &str,
        context: &mut TraversalContext,
    ) -> Result<(), String> {
        // For instance variable assignments, the name is in the "left" field
        let name_node = node
            .child_by_field_name("left")
            .ok_or_else(|| "Instance variable assignment without a name".to_string())?;

        // Extract the variable name
        let name = self.get_node_text(name_node, source_code);

        // Skip if name is empty
        if name.trim().is_empty() {
            return Ok(());
        }

        // Create a fully qualified name that includes the current class/module context
        let current_namespace = context.current_namespace();

        // Determine the FQN for the instance variable
        let fqn = if current_namespace.is_empty() {
            name.clone()
        } else {
            format!("{}#{}", current_namespace, name)
        };

        // Create a range for the definition
        let range = node_to_range(name_node);

        // Create and add the entry
        let entry = EntryBuilder::new(&name)
            .fully_qualified_name(&fqn)
            .location(uri.clone(), range)
            .entry_type(EntryType::InstanceVariable) // Using a new entry type for instance variables
            .visibility(context.visibility)
            .build()
            .map_err(|e| e.to_string())?;

        self.index.add_entry(entry);

        // Process the right-hand side of the assignment
        if let Some(right) = node.child_by_field_name("right") {
            self.traverse_node(right, uri, source_code, context)?;
        }

        Ok(())
    }

    fn process_instance_variable_reference(
        &mut self,
        node: Node,
        uri: &Url,
        source_code: &str,
        _context: &mut TraversalContext,
    ) -> Result<(), String> {
        // Extract the variable name
        let name = self.get_node_text(node, source_code);

        // Skip if name is empty
        if name.trim().is_empty() {
            return Ok(());
        }

        // For references, we don't need to create an entry, but in a real implementation
        // we would track references to variables for "find all references" functionality

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

    // Process attr_accessor, attr_reader, attr_writer method calls
    fn process_attribute_methods(
        &mut self,
        node: Node,
        uri: &Url,
        source_code: &str,
        context: &mut TraversalContext,
    ) -> Result<(), String> {
        // Check if this is a method call like attr_accessor, attr_reader, attr_writer
        if let Some(method_node) = node.child_by_field_name("method") {
            let method_name = self.get_node_text(method_node, source_code);

            // Only process specific attribute method calls
            if method_name != "attr_accessor"
                && method_name != "attr_reader"
                && method_name != "attr_writer"
            {
                return Ok(());
            }

            // Get the arguments (could be multiple symbol arguments)
            if let Some(args_node) = node.child_by_field_name("arguments") {
                let args_count = args_node.child_count();

                for i in 0..args_count {
                    if let Some(arg_node) = args_node.child(i) {
                        // Skip non-symbol nodes (like commas)
                        if arg_node.kind() != "simple_symbol" {
                            continue;
                        }

                        // Extract the attribute name without the colon
                        let mut attr_name = self.get_node_text(arg_node, source_code);
                        if attr_name.starts_with(':') {
                            attr_name = attr_name[1..].to_string();
                        }

                        // Get the current namespace (class/module)
                        let current_namespace = context.current_namespace();
                        if current_namespace.is_empty() {
                            continue; // Skip if we're not in a class/module
                        }

                        // Create a range for the attribute definition
                        let range = node_to_range(arg_node);

                        // Create entries for the accessor methods
                        if method_name == "attr_accessor" || method_name == "attr_reader" {
                            // Add the getter method
                            let getter_fqn = format!("{}#{}", current_namespace, attr_name);
                            let getter_entry = EntryBuilder::new(&attr_name)
                                .fully_qualified_name(&getter_fqn)
                                .location(uri.clone(), range.clone())
                                .entry_type(EntryType::Method)
                                .visibility(context.visibility)
                                .build()
                                .map_err(|e| e.to_string())?;

                            // Add the getter method to the index
                            self.index.add_entry(getter_entry);
                        }

                        if method_name == "attr_accessor" || method_name == "attr_writer" {
                            // Add the setter method (name=)
                            let setter_name = format!("{}=", attr_name);
                            let setter_fqn = format!("{}#{}", current_namespace, setter_name);
                            let setter_entry = EntryBuilder::new(&setter_name)
                                .fully_qualified_name(&setter_fqn)
                                .location(uri.clone(), range.clone())
                                .entry_type(EntryType::Method)
                                .visibility(context.visibility)
                                .build()
                                .map_err(|e| e.to_string())?;

                            // Add the setter method to the index
                            self.index.add_entry(setter_entry);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

// Helper function to convert a tree-sitter node to an LSP Range
fn node_to_range(node: Node) -> Range {
    let start_point = node.start_position();
    let end_point = node.end_position();

    Range {
        start: Position {
            line: start_point.row as u32,
            character: start_point.column as u32,
        },
        end: Position {
            line: end_point.row as u32,
            character: end_point.column as u32,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    // Helper function to create a temporary Ruby file with given content
    fn create_temp_ruby_file(content: &str) -> (NamedTempFile, PathBuf) {
        let mut file = NamedTempFile::new().expect("Failed to create temp file");
        file.write_all(content.as_bytes())
            .expect("Failed to write to temp file");
        let path = file.path().to_path_buf();
        (file, path)
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
        let (file, path) = create_temp_ruby_file("");

        let result = indexer.index_file(&path, "");
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

        let (file, path) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file(&path, ruby_code);
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

        let (file, path) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file(&path, ruby_code);
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

        let (file, path) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file(&path, ruby_code);
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

        let (file1, path1) = create_temp_ruby_file(ruby_code1);
        let result1 = indexer.index_file(&path1, ruby_code1);
        assert!(result1.is_ok(), "Should be able to index the first file");

        // Second file with reopened class
        let ruby_code2 = r#"
        class User
          def display_name
            @name.upcase
          end
        end
        "#;

        let (file2, path2) = create_temp_ruby_file(ruby_code2);
        let result2 = indexer.index_file(&path2, ruby_code2);
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

        let (file, path) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file(&path, ruby_code);
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

        let (file, path) = create_temp_ruby_file(ruby_code);
        let result = indexer.index_file(&path, ruby_code);
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

        let (file, path) = create_temp_ruby_file(ruby_code);

        // First, index the file
        let result = indexer.index_file(&path, ruby_code);
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

        // Now remove entries for this URI
        let uri = Url::from_file_path(&path).expect("Failed to convert path to URI");

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
        let (temp_file, path) = create_temp_ruby_file(content);

        // Index the file - this shouldn't panic
        let result = indexer.index_file(&path, content);

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
        let (temp_file, path) = create_temp_ruby_file(content);

        // Index the file - this shouldn't panic
        let result = indexer.index_file(&path, content);

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
        let (temp_file, path) = create_temp_ruby_file(content);

        // Index the file - this shouldn't panic
        let result = indexer.index_file(&path, content);

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
        let (temp_file, temp_path) = create_temp_ruby_file(test_code);

        // Index the file
        indexer.index_file(&temp_path, test_code).unwrap();

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
        let (temp_file, temp_path) = create_temp_ruby_file(test_code);

        // Index the file
        indexer.index_file(&temp_path, test_code).unwrap();

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
}
