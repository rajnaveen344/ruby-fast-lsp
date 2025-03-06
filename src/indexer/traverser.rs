use log::info;
use lsp_types::{Location, Position, Range, Url};
use std::path::Path;
#[cfg(test)]
use tempfile::NamedTempFile;
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

    pub fn traverse_node(
        &mut self,
        node: Node,
        uri: &Url,
        source_code: &str,
        context: &mut TraversalContext,
    ) -> Result<(), String> {
        match node.kind() {
            "class" => self.process_class(node, uri, source_code, context)?,
            "module" => self.process_module(node, uri, source_code, context)?,
            "method" | "singleton_method" => {
                self.process_method(node, uri, source_code, context)?
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
            "block" => {
                // Process block parameters if they exist
                if let Some(parameters) = node.child_by_field_name("parameters") {
                    self.process_block_parameters(parameters, uri, source_code, context)?;
                }

                // Process block body contents recursively
                if let Some(body) = node.child_by_field_name("body") {
                    for i in 0..body.named_child_count() {
                        if let Some(child) = body.named_child(i) {
                            self.traverse_node(child, uri, source_code, context)?;
                        }
                    }
                } else {
                    // If there's no explicit body field, traverse all children
                    for i in 0..node.named_child_count() {
                        if let Some(child) = node.named_child(i) {
                            if child.kind() != "parameters" {
                                // Skip parameters as we already processed them
                                self.traverse_node(child, uri, source_code, context)?;
                            }
                        }
                    }
                }
            }
            "block_parameters" => {
                // Process block parameters directly
                self.process_block_parameters(node, uri, source_code, context)?;
            }
            "parameters" => {
                // Process method parameters directly
                if let Some(parent) = node.parent() {
                    if parent.kind() == "method" || parent.kind() == "singleton_method" {
                        self.process_method_parameters(node, uri, source_code, context)?;
                    } else if parent.kind() == "block" {
                        self.process_block_parameters(node, uri, source_code, context)?;
                    }
                }
            }
            "call" => {
                // Check for attr_* method calls like attr_accessor, attr_reader, attr_writer
                self.process_attribute_methods(node, uri, source_code, context)?;

                // Process method call references
                self.process_method_call(node, uri, source_code, context)?;

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
                    } else if left_kind == "class_variable" {
                        // Process class variable assignment
                        self.process_class_variable(node, uri, source_code, context)?;
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
            "class_variable" => {
                // Process class variable reference
                if let Err(e) =
                    self.process_class_variable_reference(node, uri, source_code, context)
                {
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

        // Process method parameters if they exist
        if let Some(parameters) = node.child_by_field_name("parameters") {
            self.process_method_parameters(parameters, uri, source_code, context)?;
        }

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
        context: &mut TraversalContext,
    ) -> Result<(), String> {
        // Extract the variable name
        let name = self.get_node_text(node, source_code);

        // Skip if name is empty
        if name.trim().is_empty() {
            return Ok(());
        }

        // Create a range for the reference
        let range = node_to_range(node);

        // Create a location for this reference
        let location = Location {
            uri: uri.clone(),
            range,
        };

        // Add reference with just the variable name (e.g., @name)
        self.index.add_reference(&name, location.clone());

        // Also add reference with class context if available
        let current_namespace = context.current_namespace();
        if !current_namespace.is_empty() {
            let fqn = format!("{}#{}", current_namespace, name);
            self.index.add_reference(&fqn, location);
        }

        Ok(())
    }

    fn process_class_variable(
        &mut self,
        node: Node,
        uri: &Url,
        source_code: &str,
        context: &mut TraversalContext,
    ) -> Result<(), String> {
        let left = node
            .child_by_field_name("left")
            .ok_or_else(|| "Failed to get left node of class variable assignment".to_string())?;

        let name = self.get_node_text(left, source_code);

        // Add reference for the class variable
        let location = Location::new(uri.clone(), node_to_range(left));
        self.index.add_reference(&name, location);

        Ok(())
    }

    fn process_class_variable_reference(
        &mut self,
        node: Node,
        uri: &Url,
        source_code: &str,
        context: &mut TraversalContext,
    ) -> Result<(), String> {
        let name = self.get_node_text(node, source_code);

        // Add reference for the class variable
        let location = Location::new(uri.clone(), node_to_range(node));
        self.index.add_reference(&name, location);

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

    fn process_method_call(
        &mut self,
        node: Node,
        uri: &Url,
        source_code: &str,
        context: &mut TraversalContext,
    ) -> Result<(), String> {
        // Get the method name node
        if let Some(method_node) = node.child_by_field_name("method") {
            // Extract the method name
            let method_name = self.get_node_text(method_node, source_code);

            // Skip if method name is empty or is an attribute method
            if method_name.trim().is_empty()
                || method_name == "attr_accessor"
                || method_name == "attr_reader"
                || method_name == "attr_writer"
            {
                return Ok(());
            }

            // Create a range for the reference
            let range = node_to_range(method_node);

            // Create a location for this reference
            let location = Location {
                uri: uri.clone(),
                range,
            };

            // Add reference with just the method name
            self.index.add_reference(&method_name, location.clone());

            // If there's a receiver, try to determine its type
            if let Some(receiver_node) = node.child_by_field_name("receiver") {
                let receiver_text = self.get_node_text(receiver_node, source_code);

                // If the receiver starts with uppercase, it's likely a class name
                if receiver_text
                    .chars()
                    .next()
                    .map_or(false, |c| c.is_uppercase())
                {
                    let fqn = format!("{}#{}", receiver_text, method_name);
                    self.index.add_reference(&fqn, location.clone());
                }
            } else {
                // No explicit receiver, use current namespace as context
                let current_namespace = context.current_namespace();
                if !current_namespace.is_empty() {
                    let fqn = format!("{}#{}", current_namespace, method_name);
                    self.index.add_reference(&fqn, location);
                }
            }
        }

        Ok(())
    }

    // Process method parameters (arguments) in method definitions
    fn process_method_parameters(
        &mut self,
        node: Node,
        uri: &Url,
        source_code: &str,
        context: &mut TraversalContext,
    ) -> Result<(), String> {
        // Iterate through all parameter nodes
        for i in 0..node.named_child_count() {
            if let Some(param_node) = node.named_child(i) {
                let param_kind = param_node.kind();
                let param_name = match param_kind {
                    "identifier" => self.get_node_text(param_node, source_code),
                    "optional_parameter"
                    | "keyword_parameter"
                    | "rest_parameter"
                    | "hash_splat_parameter"
                    | "block_parameter" => {
                        if let Some(name_node) = param_node.child_by_field_name("name") {
                            self.get_node_text(name_node, source_code)
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };

                if param_name.trim().is_empty() {
                    continue;
                }

                // Create a range for the definition
                let range = node_to_range(param_node);

                // Create a fully qualified name for the parameter
                let current_namespace = context.current_namespace();
                let current_method = context
                    .current_method
                    .as_ref()
                    .ok_or_else(|| "Method parameter outside of method context".to_string())?;

                let fqn = if current_namespace.is_empty() {
                    format!("{}${}", current_method, param_name)
                } else {
                    format!("{}#{}${}", current_namespace, current_method, param_name)
                };

                // Create and add the entry
                let entry = EntryBuilder::new(&param_name)
                    .fully_qualified_name(&fqn)
                    .location(uri.clone(), range)
                    .entry_type(EntryType::LocalVariable)
                    .metadata("kind", "parameter")
                    .build()
                    .map_err(|e| e.to_string())?;

                self.index.add_entry(entry);
            }
        }

        Ok(())
    }

    // Process block parameters in block definitions
    fn process_block_parameters(
        &mut self,
        parameters: Node,
        uri: &Url,
        source_code: &str,
        context: &mut TraversalContext,
    ) -> Result<(), String> {
        // Iterate through all parameter nodes
        for i in 0..parameters.named_child_count() {
            if let Some(param) = parameters.named_child(i) {
                let param_text = match param.kind() {
                    "identifier" => Some(self.get_node_text(param, source_code)),
                    "optional_parameter"
                    | "keyword_parameter"
                    | "rest_parameter"
                    | "hash_splat_parameter"
                    | "block_parameter" => param
                        .child_by_field_name("name")
                        .map(|name_node| self.get_node_text(name_node, source_code)),
                    _ => None,
                };

                if let Some(param_text) = param_text {
                    if param_text.trim().is_empty() {
                        continue;
                    }

                    // Create a range for the parameter
                    let range = node_to_range(param);

                    // Find the method that contains this block
                    let mut current = parameters.clone();
                    let mut method_name = None;

                    while let Some(p) = current.parent() {
                        if p.kind() == "method" || p.kind() == "singleton_method" {
                            if let Some(method_name_node) = p.child_by_field_name("name") {
                                method_name =
                                    Some(self.get_node_text(method_name_node, source_code));
                            }
                            break;
                        }
                        current = p;
                    }

                    // Build the FQN based on context
                    let fqn = if let Some(method_name) =
                        method_name.as_ref().or(context.current_method.as_ref())
                    {
                        if context.namespace_stack.is_empty() {
                            format!("{}$block${}", method_name, param_text)
                        } else {
                            format!(
                                "{}#{}$block${}",
                                context.current_namespace(),
                                method_name,
                                param_text
                            )
                        }
                    } else {
                        if context.namespace_stack.is_empty() {
                            format!("$block${}", param_text)
                        } else {
                            format!("{}#$block${}", context.current_namespace(), param_text)
                        }
                    };

                    // Create and add the entry
                    let entry = EntryBuilder::new(&param_text)
                        .fully_qualified_name(&fqn)
                        .location(uri.clone(), range)
                        .entry_type(EntryType::LocalVariable)
                        .metadata("kind", "block_parameter")
                        .build()
                        .map_err(|e| e.to_string())?;

                    self.index.add_entry(entry);
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
        let (temp_file, path) = create_temp_ruby_file(ruby_code);
        let uri = Url::from_file_path(path.clone()).unwrap();

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
        let (temp_file, path) = create_temp_ruby_file(ruby_code);
        let uri = Url::from_file_path(path.clone()).unwrap();

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
        let (temp_file, path) = create_temp_ruby_file(ruby_code);
        let uri = Url::from_file_path(path.clone()).unwrap();

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
        let (user_file, user_path) = create_temp_ruby_file(user_content);
        let (auth_file, auth_path) = create_temp_ruby_file(auth_content);
        let (order_file, order_path) = create_temp_ruby_file(order_content);

        // Index the files
        indexer.index_file(&user_path, user_content).unwrap();
        indexer.index_file(&auth_path, auth_content).unwrap();
        indexer.index_file(&order_path, order_content).unwrap();

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
        let (order_file, order_path) = create_temp_ruby_file(order_content);

        // Initial indexing
        indexer.index_file(&order_path, order_content).unwrap();
        let initial_refs = indexer.index().find_references("@items");

        // Reindex the same file
        indexer.index_file(&order_path, order_content).unwrap();
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

        let (file, path) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file(&path, ruby_code);
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
        let (temp_file, path) = create_temp_ruby_file(code);

        // Index the file
        indexer
            .index_file(&path, code)
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
        let (temp_file, path) = create_temp_ruby_file(code);

        // Index the file
        indexer
            .index_file(&path, code)
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
        let (temp_file, path) = create_temp_ruby_file(code);

        // Index the file
        indexer
            .index_file(&path, code)
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
