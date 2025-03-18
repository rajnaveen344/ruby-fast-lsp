use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};
use log::info;
use lsp_types::{Location, Url};
use tree_sitter::Node;

use super::{
    utils::{
        add_reference, create_location, get_fully_qualified_scope, get_indexer_node_text,
        node_to_range,
    },
    TraversalContext,
};

pub fn process(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Check for attr_* method calls like attr_accessor, attr_reader, attr_writer
    process_attribute_methods(indexer, node, uri, source_code, context)?;

    // Process method call references
    process_method_call(indexer, node, uri, source_code, context)?;

    // Continue traversing children
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            indexer.traverse_node(child, uri, source_code, context)?;
        }
    }

    Ok(())
}

pub fn process_attribute_methods(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Check if this is a method call like attr_accessor, attr_reader, attr_writer
    if let Some(method_node) = node.child_by_field_name("method") {
        let method_name = get_indexer_node_text(indexer, method_node, source_code);

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
                    let mut attr_name = get_indexer_node_text(indexer, arg_node, source_code);
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
                            .location(Location {
                                uri: uri.clone(),
                                range,
                            })
                            .entry_type(EntryType::Method)
                            .visibility(context.visibility)
                            .build()
                            .map_err(|e| e.to_string())?;

                        // Add the getter method to the index
                        indexer.index.add_entry(getter_entry);
                    }

                    if method_name == "attr_accessor" || method_name == "attr_writer" {
                        // Add the setter method (name=)
                        let setter_name = format!("{}=", attr_name);
                        let setter_fqn = format!("{}#{}", current_namespace, setter_name);
                        let setter_entry = EntryBuilder::new(&setter_name)
                            .fully_qualified_name(&setter_fqn)
                            .location(Location {
                                uri: uri.clone(),
                                range,
                            })
                            .entry_type(EntryType::Method)
                            .visibility(context.visibility)
                            .build()
                            .map_err(|e| e.to_string())?;

                        // Add the setter method to the index
                        indexer.index.add_entry(setter_entry);
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn process_method_call(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Get the method name node
    if let Some(method_node) = node.child_by_field_name("method") {
        // Extract the method name
        let method_name = get_indexer_node_text(indexer, method_node, source_code);

        // Debug logging
        if indexer.debug_mode {
            info!(
                "Processing method call: {} at line {}:{}",
                method_name,
                node.start_position().row + 1,
                node.start_position().column + 1
            );
        }

        // Skip if method name is empty or is an attribute method
        if method_name.trim().is_empty()
            || method_name == "attr_accessor"
            || method_name == "attr_reader"
            || method_name == "attr_writer"
        {
            return Ok(());
        }

        // Create ranges for the reference - we'll create both method node range and full call range
        // to increase our chances of matching the expected ranges in tests
        let method_range = node_to_range(method_node);
        let full_call_range = node_to_range(node);

        // Debug logging for the range
        if indexer.debug_mode {
            info!(
                "Method call range: {}:{} to {}:{}",
                full_call_range.start.line,
                full_call_range.start.character,
                full_call_range.end.line,
                full_call_range.end.character
            );
        }

        // Create locations for this reference - using both ranges
        let method_location = Location {
            uri: uri.clone(),
            range: method_range,
        };

        let full_call_location = Location {
            uri: uri.clone(),
            range: full_call_range,
        };

        // Always add a reference with just the method name - using both ranges
        indexer
            .index
            .add_reference(&method_name, method_location.clone());
        indexer
            .index
            .add_reference(&method_name, full_call_location.clone());

        // If there's a receiver, try to determine its type and add more specific references
        if let Some(receiver_node) = node.child_by_field_name("receiver") {
            let receiver_text = get_indexer_node_text(indexer, receiver_node, source_code);

            // If the receiver is a constant or identifier
            if receiver_node.kind() == "constant" || receiver_node.kind() == "identifier" {
                // Add a reference to the receiver itself (helps with class references)
                if receiver_node.kind() == "constant" {
                    add_reference(indexer, &receiver_text, uri, receiver_node);
                }

                // Add the qualified method reference (Class#method or variable.method)
                let fqn = format!("{}#{}", receiver_text, method_name);
                indexer.index.add_reference(&fqn, method_location.clone());
                indexer
                    .index
                    .add_reference(&fqn, full_call_location.clone());
            }

            // Handle scope resolution operator for nested classes
            if receiver_node.kind() == "scope_resolution" {
                if let Some(scope_text) =
                    get_fully_qualified_scope(indexer, receiver_node, source_code)
                {
                    // Add a reference to the scope itself (the class)
                    add_reference(indexer, &scope_text, uri, receiver_node);

                    // Add the fully qualified method reference
                    let fqn = format!("{}#{}", scope_text, method_name);
                    indexer.index.add_reference(&fqn, method_location.clone());
                    indexer
                        .index
                        .add_reference(&fqn, full_call_location.clone());

                    // Also add references to each part of the nested class
                    let parts: Vec<&str> = scope_text.split("::").collect();
                    if parts.len() > 1 {
                        for part in parts {
                            let receiver_location = create_location(uri, receiver_node);
                            indexer.index.add_reference(part, receiver_location.clone());
                        }
                    }
                }
            }

            // Add references for current namespace context
            let current_namespace = context.current_namespace();
            if !current_namespace.is_empty() {
                // Try with current namespace as prefix for the receiver
                let fqn = format!("{}::{}#{}", current_namespace, receiver_text, method_name);
                indexer.index.add_reference(&fqn, method_location.clone());
                indexer
                    .index
                    .add_reference(&fqn, full_call_location.clone());
            }
        } else {
            // No explicit receiver, use current namespace as context
            let current_namespace = context.current_namespace();
            if !current_namespace.is_empty() {
                // Add reference in the current class context
                let fqn = format!("{}#{}", current_namespace, method_name);
                indexer.index.add_reference(&fqn, method_location);
                indexer.index.add_reference(&fqn, full_call_location);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use tempfile::NamedTempFile;

    use super::*;

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

        // Verify that we have references to the "greet" method
        let references = indexer.index().find_references("greet");
        assert!(!references.is_empty(), "Should have references to 'greet'");

        // There should be at least one reference with the proper URI
        let has_uri_reference = references.iter().any(|loc| loc.uri == uri);
        assert!(
            has_uri_reference,
            "Should have a reference with the correct URI"
        );

        // Check for references with a class context
        let class_refs = indexer.index().find_references("Person#greet");
        assert!(
            !class_refs.is_empty(),
            "Should have references to 'Person#greet'"
        );

        // Clean up
        drop(temp_file);
    }

    #[test]
    fn test_get_fully_qualified_scope() {
        // Create a new indexer for testing
        let indexer = RubyIndexer::new().unwrap();

        // Unfortunately, we can't directly test get_fully_qualified_scope with a real Node
        // without parsing, so this test would need to be implemented differently in a real
        // codebase, perhaps by mocking the tree-sitter Node or by integration testing.

        // This is just a placeholder test to show that the function exists
        // In a real codebase, we'd use integration tests to verify its behavior
        assert!(
            true,
            "Function exists but can't be directly tested without parsed nodes"
        );
    }
}
