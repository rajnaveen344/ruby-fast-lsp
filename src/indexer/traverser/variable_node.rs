use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};
use lsp_types::{Location, Url};
use tree_sitter::Node;

use super::{
    utils::{add_reference, create_location, get_indexer_node_text, node_to_range},
    TraversalContext,
};

pub fn process_local_variable(
    indexer: &mut RubyIndexer,
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
    let name = get_indexer_node_text(indexer, name_node, source_code);

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
        .location(Location {
            uri: uri.clone(),
            range,
        })
        .entry_type(EntryType::LocalVariable)
        .build()
        .map_err(|e| e.to_string())?;

    indexer.index.add_entry(entry);

    // Continue traversing the right side of the assignment
    if let Some(right) = node.child_by_field_name("right") {
        indexer.traverse_node(right, uri, source_code, context)?;
    }

    Ok(())
}

pub fn process_instance_variable(
    indexer: &mut RubyIndexer,
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
    let name = get_indexer_node_text(indexer, name_node, source_code);

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
        .location(Location {
            uri: uri.clone(),
            range,
        })
        .entry_type(EntryType::InstanceVariable)
        .visibility(context.visibility)
        .build()
        .map_err(|e| e.to_string())?;

    indexer.index.add_entry(entry);

    // Process the right-hand side of the assignment
    if let Some(right) = node.child_by_field_name("right") {
        indexer.traverse_node(right, uri, source_code, context)?;
    }

    Ok(())
}

pub fn process_instance_variable_reference(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Extract the variable name
    let name = get_indexer_node_text(indexer, node, source_code);

    // Skip if name is empty
    if name.trim().is_empty() {
        return Ok(());
    }

    // Add reference with just the variable name (e.g., @name)
    add_reference(indexer, &name, uri, node);

    // Also add reference with class context if available
    let current_namespace = context.current_namespace();
    if !current_namespace.is_empty() {
        let fqn = format!("{}#{}", current_namespace, name);
        let location = create_location(uri, node);
        indexer.index.add_reference(&fqn, location);
    }

    Ok(())
}

pub fn process_class_variable(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    _context: &mut TraversalContext,
) -> Result<(), String> {
    let left = node
        .child_by_field_name("left")
        .ok_or_else(|| "Failed to get left node of class variable assignment".to_string())?;

    let name = get_indexer_node_text(indexer, left, source_code);

    // Add reference for the class variable
    add_reference(indexer, &name, uri, left);

    Ok(())
}

pub fn process_class_variable_reference(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    _context: &mut TraversalContext,
) -> Result<(), String> {
    // Get the variable name
    let name = get_indexer_node_text(indexer, node, source_code);

    // Add reference for the class variable
    add_reference(indexer, &name, uri, node);

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
    fn test_local_variable_processing() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r##"
        def process_data
          result = "hello"
          final = result.upcase
          return final
        end

        def another_method
          # Same name but different scope
          result = 42
          result += 10
        end
        "##;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Check if entries were indexed
        let entries = &indexer.index().entries;
        assert!(!entries.is_empty(), "Should have indexed entries");

        // Clean up
        drop(file);
    }

    #[test]
    fn test_instance_variable_processing() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r##"
        class Person
          def initialize(name, age)
            @name = name
            @age = age
          end

          def greet
            "Hello, " + @name + "! You are " + @age.to_s + " years old."
          end

          def birthday
            @age += 1
          end
        end
        "##;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Check for instance variable entries
        let entries = &indexer.index().entries;
        assert!(!entries.is_empty(), "Should have indexed entries");

        // Clean up
        drop(file);
    }

    #[test]
    fn test_class_variable_processing() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r##"
        class Counter
          @@count = 0

          def self.increment
            @@count += 1
          end

          def self.current_count
            @@count
          end

          def report
            "Current count: " + @@count.to_s
          end
        end
        "##;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Check if entries were indexed
        let entries = &indexer.index().entries;
        assert!(!entries.is_empty(), "Should have indexed entries");

        // Clean up
        drop(file);
    }

    #[test]
    fn test_nested_variable_scopes() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r##"
        class ShoppingCart
          @@tax_rate = 0.08

          def initialize(items = [])
            @items = items
            @total = 0
          end

          def calculate_total
            subtotal = 0
            @items.each do |item|
              price = item[:price]
              subtotal += price
            end

            # Local and instance variables interacting
            @total = subtotal * (1 + @@tax_rate)

            return @total
          end
        end
        "##;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Check if entries were indexed
        let entries = &indexer.index().entries;
        assert!(!entries.is_empty(), "Should have indexed entries");

        // Clean up
        drop(file);
    }
}
