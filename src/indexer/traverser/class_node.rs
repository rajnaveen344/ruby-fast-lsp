use lsp_types::{Location, Url};
use tree_sitter::Node;

use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};

use super::{
    utils::{add_reference, create_location, get_fqn, get_indexer_node_text, node_to_range},
    TraversalContext,
};

pub fn process(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    content: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    let current_namespace = context.current_namespace();
    let class_name = get_class_name(indexer, node, content);
    let class_fqn = get_fqn(&current_namespace, &class_name);
    let range = node_to_range(node);

    // Find the class name node for reference
    if let Some(name_node) = node.child_by_field_name("name") {
        // Add reference to the class name
        add_reference(indexer, &class_name, uri, name_node);

        // Add reference to fully qualified name if different
        if class_name != class_fqn {
            let location = create_location(uri, name_node);
            indexer.index.add_reference(&class_fqn, location);
        }
    }

    let location = Location {
        uri: uri.clone(),
        range,
    };

    let entry = EntryBuilder::new(&class_name)
        .fully_qualified_name(&class_fqn)
        .location(location)
        .entry_type(EntryType::Class)
        .build()?;

    // Add entry to index
    indexer.index.add_entry(entry);

    // Add to namespace tree
    let parent_namespace = if context.namespace_stack.is_empty() {
        String::new()
    } else {
        current_namespace
    };

    let children = indexer
        .index
        .namespace_tree
        .entry(parent_namespace)
        .or_insert_with(Vec::new);

    if !children.contains(&class_fqn) {
        children.push(class_fqn.clone());
    }

    // Push class to namespace stack
    context.namespace_stack.push(class_name);

    // Process class body
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.named_child_count() {
            if let Some(child) = body.named_child(i) {
                indexer.traverse_node(child, uri, content, context)?;
            }
        }
    }

    // Pop class from namespace stack
    context.namespace_stack.pop();

    Ok(())
}

fn get_class_name(indexer: &RubyIndexer, node: Node, content: &str) -> String {
    if let Some(name_node) = node.child_by_field_name("name") {
        get_indexer_node_text(indexer, name_node, content)
    } else {
        // Fallback in case we can't find the name node
        "UnknownClass".to_string()
    }
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
        assert_eq!(
            person_entries.unwrap()[0].entry_type,
            EntryType::Class,
            "Person should be indexed as a class"
        );

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
        assert_eq!(
            inner_entries.unwrap()[0].entry_type,
            EntryType::Class,
            "Inner should be indexed as a class"
        );

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
            "Root namespace should contain Outer"
        );

        // Keep file in scope until end of test
        drop(file);
    }
}
