use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};
use lsp_types::{Location, Url};
use tree_sitter::Node;

use super::{
    parameter_node,
    utils::{add_reference, create_location, get_indexer_node_text, node_to_range},
    TraversalContext,
};

pub fn process(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Find the method name node
    let name_node = node
        .child_by_field_name("name")
        .ok_or_else(|| "Method without a name".to_string())?;

    // Extract method name information
    let (name, method_name, fqn) = extract_method_info(indexer, name_node, source_code, context);

    // Add references for method name
    add_method_references(indexer, &name, &fqn, uri, name_node, context);

    // Create and add the method entry
    let entry = create_method_entry(node, &name, &fqn, uri, context)?;
    indexer.index.add_entry(entry);

    // Process method body and parameters
    process_method_contents(indexer, node, uri, source_code, context, method_name)?;

    Ok(())
}

fn extract_method_info(
    indexer: &RubyIndexer,
    name_node: Node,
    source_code: &str,
    context: &TraversalContext,
) -> (String, String, String) {
    // Extract the name text
    let name = get_indexer_node_text(indexer, name_node, source_code);

    // Create a fully qualified name
    let current_namespace = context.current_namespace();
    let method_name = name.clone();

    let fqn = if current_namespace.is_empty() {
        method_name.clone()
    } else {
        format!("{}#{}", current_namespace, method_name)
    };

    (name, method_name, fqn)
}

fn add_method_references(
    indexer: &mut RubyIndexer,
    name: &str,
    fqn: &str,
    uri: &Url,
    name_node: Node,
    context: &TraversalContext,
) {
    // Add reference to the method name
    add_reference(indexer, name, uri, name_node);

    // Add reference to the fully qualified name if in a namespace
    let current_namespace = context.current_namespace();
    if !current_namespace.is_empty() {
        let location = create_location(uri, name_node);
        indexer.index.add_reference(fqn, location);
    }
}

fn create_method_entry(
    node: Node,
    name: &str,
    fqn: &str,
    uri: &Url,
    context: &TraversalContext,
) -> Result<crate::indexer::entry::Entry, String> {
    // Create a range for the definition
    let range = node_to_range(node);

    // Create a method type based on node kind
    let entry_type = if node.kind() == "singleton_method" {
        EntryType::Method // We could use a "ClassMethod" type if we had one
    } else {
        EntryType::Method
    };

    // Create the entry
    EntryBuilder::new(name)
        .fully_qualified_name(fqn)
        .location(Location {
            uri: uri.clone(),
            range,
        })
        .entry_type(entry_type)
        .visibility(context.visibility)
        .build()
        .map_err(|e| e.to_string())
}

fn process_method_contents(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
    method_name: String,
) -> Result<(), String> {
    // Record the method name in the context for parameter and variable scoping
    let previous_method = context.current_method.clone();
    context.current_method = Some(method_name);

    // Process method parameters if present
    process_method_parameters(indexer, node, uri, source_code, context)?;

    // Process method body
    process_method_body(indexer, node, uri, source_code, context)?;

    // Restore previous method context
    context.current_method = previous_method;

    Ok(())
}

fn process_method_parameters(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    if let Some(parameters) = node.child_by_field_name("parameters") {
        parameter_node::process_method_parameters(indexer, parameters, uri, source_code, context)?;
    }
    Ok(())
}

fn process_method_body(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.named_child_count() {
            if let Some(child) = body.named_child(i) {
                indexer.traverse_node(child, uri, source_code, context)?;
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
    fn test_method_processing() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r#"
        class Calculator
          def add(a, b)
            a + b
          end

          def subtract(a, b)
            a - b
          end

          def self.version
            "1.0.0"
          end

          # Method with special characters
          def +(other)
            self.value + other.value
          end

          private

          def calculate_tax(amount, rate)
            amount * rate
          end
        end
        "#;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Get the index
        let index = indexer.index();

        // Check that methods were indexed with their FQNs
        let add_entries = index.methods_by_name.get("add");
        assert!(add_entries.is_some(), "add method should be indexed");

        let subtract_entries = index.methods_by_name.get("subtract");
        assert!(
            subtract_entries.is_some(),
            "subtract method should be indexed"
        );

        let version_entries = index.methods_by_name.get("version");
        assert!(
            version_entries.is_some(),
            "version class method should be indexed"
        );

        let plus_entries = index.methods_by_name.get("+");
        assert!(plus_entries.is_some(), "+ method should be indexed");

        let tax_entries = index.methods_by_name.get("calculate_tax");
        assert!(
            tax_entries.is_some(),
            "calculate_tax method should be indexed"
        );

        // Clean up
        drop(file);
    }

    #[test]
    fn test_method_parameters() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r#"
        def greet(name, age = nil, options = {})
          greeting = "Hello, " + name + "!"
          greeting += " You are " + age.to_s + " years old." if age

          if options[:formal]
            greeting + " It is a pleasure to meet you."
          else
            greeting
          end
        end
        "#;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Verify the method is indexed
        let index = indexer.index();
        let greet_entries = index.methods_by_name.get("greet");
        assert!(greet_entries.is_some(), "greet method should be indexed");

        // Cannot directly check parameters here as they're not stored directly
        // under the method but are processed separately by parameter_node

        // Clean up
        drop(file);
    }
}
