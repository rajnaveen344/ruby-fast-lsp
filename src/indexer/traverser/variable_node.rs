use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};
use log::info;
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
    // Extract the name node
    let name_node = extract_variable_name_node(node)?;
    let var_name = get_indexer_node_text(indexer, name_node, source_code);

    // Skip if name is empty
    if var_name.trim().is_empty() {
        return Ok(());
    }

    // Create fully qualified name with method context if available
    let fqn = create_variable_fqn(&var_name, context);

    // Debug logging
    log_variable_processing(indexer, "local", &var_name, &fqn, name_node);

    // Create and add references
    add_variable_references(indexer, &var_name, &fqn, uri, name_node);

    // Process the right-hand side of the assignment
    process_assignment_rhs(indexer, node, uri, source_code, context)?;

    Ok(())
}

pub fn process_instance_variable(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Extract the name node
    let name_node = extract_variable_name_node(node)?;
    let var_name = get_indexer_node_text(indexer, name_node, source_code);

    // Skip if name is empty or doesn't start with @
    if var_name.trim().is_empty() || !var_name.starts_with('@') {
        return Ok(());
    }

    // Create fully qualified name with class context
    let fqn = create_instance_variable_fqn(&var_name, context);

    // Debug logging
    log_variable_processing(indexer, "instance", &var_name, &fqn, name_node);

    // Create and add references
    add_variable_references(indexer, &var_name, &fqn, uri, name_node);

    // Process the right-hand side of the assignment
    process_assignment_rhs(indexer, node, uri, source_code, context)?;

    Ok(())
}

pub fn process_class_variable(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Extract the name node
    let name_node = extract_variable_name_node(node)?;
    let var_name = get_indexer_node_text(indexer, name_node, source_code);

    // Skip if name is empty or doesn't start with @@
    if var_name.trim().is_empty() || !var_name.starts_with("@@") {
        return Ok(());
    }

    // Create fully qualified name with class context
    let fqn = create_class_variable_fqn(&var_name, context);

    // Debug logging
    log_variable_processing(indexer, "class", &var_name, &fqn, name_node);

    // Create and add references
    add_variable_references(indexer, &var_name, &fqn, uri, name_node);

    // Process the right-hand side of the assignment
    process_assignment_rhs(indexer, node, uri, source_code, context)?;

    Ok(())
}

fn extract_variable_name_node(node: Node) -> Result<Node, String> {
    // Extract the variable name node from an assignment
    match node.child_by_field_name("left") {
        Some(left_node) => Ok(left_node),
        None => Err("Variable node missing left field".to_string()),
    }
}

fn create_variable_fqn(var_name: &str, context: &TraversalContext) -> String {
    // Create a fully qualified name for a local variable
    if let Some(method_name) = &context.current_method {
        // If we're in a method, qualify with method name
        let current_ns = context.current_namespace();
        if current_ns.is_empty() {
            format!("{}#{}", method_name, var_name)
        } else {
            format!("{}#{}#{}", current_ns, method_name, var_name)
        }
    } else {
        // Otherwise use just the variable name
        var_name.to_string()
    }
}

fn create_instance_variable_fqn(var_name: &str, context: &TraversalContext) -> String {
    // Create a fully qualified name for an instance variable
    let current_ns = context.current_namespace();
    if current_ns.is_empty() {
        var_name.to_string()
    } else {
        format!("{}#{}", current_ns, var_name)
    }
}

fn create_class_variable_fqn(var_name: &str, context: &TraversalContext) -> String {
    // Create a fully qualified name for a class variable
    let current_ns = context.current_namespace();
    if current_ns.is_empty() {
        var_name.to_string()
    } else {
        format!("{}#{}", current_ns, var_name)
    }
}

fn log_variable_processing(
    indexer: &RubyIndexer,
    var_type: &str,
    var_name: &str,
    fqn: &str,
    node: Node,
) {
    // Log variable processing if debug mode is enabled
    if indexer.debug_mode {
        info!(
            "Processing {} variable: {} (FQN: {}) at {}:{}",
            var_type,
            var_name,
            fqn,
            node.start_position().row + 1,
            node.start_position().column + 1
        );
    }
}

fn add_variable_references(
    indexer: &mut RubyIndexer,
    var_name: &str,
    fqn: &str,
    uri: &Url,
    node: Node,
) {
    // Add references for both the variable name and its fully qualified form
    add_reference(indexer, var_name, uri, node);
    add_reference(indexer, fqn, uri, node);
}

fn process_assignment_rhs(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Process the right side of the assignment if it exists
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
    context: &TraversalContext,
) -> Result<(), String> {
    // Extract the variable name
    let var_name = get_indexer_node_text(indexer, node, source_code);

    // Skip if name is empty or invalid
    if var_name.trim().is_empty() || !var_name.starts_with('@') {
        return Ok(());
    }

    // Create the fully qualified name
    let fqn = create_instance_variable_fqn(&var_name, context);

    // Debug logging
    log_variable_processing(indexer, "instance", &var_name, &fqn, node);

    // Add references
    add_variable_references(indexer, &var_name, &fqn, uri, node);

    Ok(())
}

pub fn process_class_variable_reference(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &TraversalContext,
) -> Result<(), String> {
    // Extract the variable name
    let var_name = get_indexer_node_text(indexer, node, source_code);

    // Skip if name is empty or invalid
    if var_name.trim().is_empty() || !var_name.starts_with("@@") {
        return Ok(());
    }

    // Create the fully qualified name
    let fqn = create_class_variable_fqn(&var_name, context);

    // Debug logging
    log_variable_processing(indexer, "class", &var_name, &fqn, node);

    // Add references
    add_variable_references(indexer, &var_name, &fqn, uri, node);

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
        // Create a new indexer
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        indexer.set_debug_mode(true);

        // Define test data with local variables
        let ruby_code = r##"
def process_data(input)
  # Local variable
  result = input * 2

  # Use the variable
  final = result + 10

  return final
end
"##;

        // Create a temporary file
        let (file, uri) = create_temp_ruby_file(ruby_code);

        // Index the file
        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Verify the entries were indexed
        let entries = indexer.index().entries.len();
        assert!(entries > 0, "Should have indexed some entries");

        // Clean up
        drop(file);
    }

    #[test]
    fn test_instance_variable_processing() {
        // Create a new indexer
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        indexer.set_debug_mode(true);

        // Define test data with instance variables
        let ruby_code = r##"
class Person
  def initialize(name)
    @name = name
  end

  def greet
    puts "Hello, " + @name
  end
end
"##;

        // Create a temporary file
        let (file, uri) = create_temp_ruby_file(ruby_code);

        // Index the file
        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Verify the entries were indexed
        let entries = indexer.index().entries.len();
        assert!(entries > 0, "Should have indexed some entries");

        // Clean up
        drop(file);
    }

    #[test]
    fn test_class_variable_processing() {
        // Create a new indexer
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        indexer.set_debug_mode(true);

        // Define test data with class variables
        let ruby_code = r##"
class Logger
  @@log_level = "INFO"

  def self.log(message)
    puts "[" + @@log_level + "] " + message
  end

  def self.set_level(level)
    @@log_level = level
  end
end
"##;

        // Create a temporary file
        let (file, uri) = create_temp_ruby_file(ruby_code);

        // Index the file
        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Verify the entries were indexed
        let entries = indexer.index().entries.len();
        assert!(entries > 0, "Should have indexed some entries");

        // Clean up
        drop(file);
    }

    #[test]
    fn test_nested_variable_scopes() {
        // Create a new indexer
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        indexer.set_debug_mode(true);

        // Define test data with nested scopes and variables
        let ruby_code = r##"
module TestModule
  @@module_var = "module value"

  class TestClass
    @@class_var = "class value"

    def instance_method
      @instance_var = "instance value"
      local_var = "local value"

      puts @instance_var
      puts local_var
      puts @@class_var
      puts @@module_var
    end
  end
end
"##;

        // Create a temporary file
        let (file, uri) = create_temp_ruby_file(ruby_code);

        // Index the file
        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Verify the entries were indexed
        let entries = indexer.index().entries.len();
        assert!(entries > 0, "Should have indexed some entries");

        // Clean up
        drop(file);
    }
}
