use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};
use lsp_types::{Location, Url};
use tree_sitter::Node;

use super::{parameter_node, utils::node_to_range, TraversalContext};

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

    // Extract the name text
    let name = indexer.get_node_text(name_node, source_code);

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

    // Create a range for the method name reference
    let name_range = node_to_range(name_node);

    // Create a location for this reference
    let location = Location {
        uri: uri.clone(),
        range: name_range,
    };

    // Add reference to the method name
    indexer.index.add_reference(&name, location.clone());

    // Add reference to the fully qualified name
    if name != fqn {
        indexer.index.add_reference(&fqn, location.clone());
    }

    // Also add a reference to the method declaration itself
    // This is important for finding references to method declarations
    let declaration_location = Location {
        uri: uri.clone(),
        range,
    };
    indexer
        .index
        .add_reference(&name, declaration_location.clone());
    if name != fqn {
        indexer.index.add_reference(&fqn, declaration_location);
    }

    // Create and add the entry
    let entry = EntryBuilder::new(&name)
        .fully_qualified_name(&fqn)
        .location(Location {
            uri: uri.clone(),
            range,
        })
        .entry_type(EntryType::Method)
        .visibility(context.visibility)
        .build()
        .map_err(|e| e.to_string())?;

    indexer.index.add_entry(entry);

    // Set the current method before processing the body
    context.current_method = Some(name.clone());

    // Process method parameters if they exist
    if let Some(parameters) = node.child_by_field_name("parameters") {
        parameter_node::process(indexer, parameters, uri, source_code, context)?;
    }

    // Process method body contents recursively
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.named_child_count() {
            if let Some(child) = body.named_child(i) {
                indexer.traverse_node(child, uri, source_code, context)?;
            }
        }
    }

    // Reset the current method after processing
    context.current_method = None;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use tempfile::NamedTempFile;

    use crate::indexer::entry::EntryType;

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
        class Person
          def initialize(name, age)
            @name = name
            @age = age
          end

          def greet
            "Hello, #{@name}!"
          end

          def self.create(name, age)
            new(name, age)
          end
        end
        "#;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
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

        // Verify instance method
        let greet_entries = index.methods_by_name.get("greet");
        assert!(greet_entries.is_some(), "greet method should be indexed");
        assert_eq!(
            greet_entries.unwrap()[0].entry_type,
            EntryType::Method,
            "greet should be indexed as a method"
        );
        assert_eq!(
            greet_entries.unwrap()[0].fully_qualified_name,
            "Person#greet",
            "fully qualified name should include class name"
        );

        // Verify class method
        let create_entries = index.methods_by_name.get("create");
        assert!(create_entries.is_some(), "create method should be indexed");
        assert_eq!(
            create_entries.unwrap()[0].entry_type,
            EntryType::Method,
            "create should be indexed as a method"
        );
        assert_eq!(
            create_entries.unwrap()[0].fully_qualified_name,
            "Person#create",
            "fully qualified name should include class name"
        );

        // Verify references were indexed
        let initialize_refs = index.find_references("initialize");
        assert!(
            !initialize_refs.is_empty(),
            "Should have references to initialize"
        );

        let person_initialize_refs = index.find_references("Person#initialize");
        assert!(
            !person_initialize_refs.is_empty(),
            "Should have references to Person#initialize"
        );

        // Clean up
        drop(file);
    }

    #[test]
    fn test_method_parameters() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r#"
        class Calculator
          def add(a, b)
            a + b
          end

          def multiply(a, b = 1)
            a * b
          end

          def divide(a, b, **options)
            result = a / b
            result = result.round(options[:precision]) if options[:precision]
            result
          end
        end
        "#;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri, ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        let index = indexer.index();

        // Verify methods were indexed
        let add_entries = index.methods_by_name.get("add");
        assert!(add_entries.is_some(), "add method should be indexed");

        let multiply_entries = index.methods_by_name.get("multiply");
        assert!(
            multiply_entries.is_some(),
            "multiply method should be indexed"
        );

        let divide_entries = index.methods_by_name.get("divide");
        assert!(divide_entries.is_some(), "divide method should be indexed");

        // Clean up
        drop(file);
    }
}
