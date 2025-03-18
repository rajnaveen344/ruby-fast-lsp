use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};
use log::info;
use lsp_types::{Location, Url};
use tree_sitter::Node;

use super::{
    utils::{add_reference, get_indexer_node_text, node_to_range},
    TraversalContext,
};

pub fn process(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Add a flag to indicate we're processing parameters
    // This helps with skipping certain operations in other parts of the traversal
    let current_method = context.current_method.clone();

    // Process based on the parent type
    let parent = node.parent();
    if let Some(parent_node) = parent {
        match parent_node.kind() {
            "method" | "singleton_method" => {
                process_method_parameters(indexer, node, uri, source_code, context)?;
            }
            "block" => {
                process_block_parameters(indexer, node, uri, source_code, context)?;
            }
            _ => {}
        }
    }

    // Also process the child parameters
    traverse_parameter_children(indexer, node, uri, source_code, context)?;

    Ok(())
}

fn traverse_parameter_children(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Process all child nodes
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            indexer.traverse_node(child, uri, source_code, context)?;
        }
    }
    Ok(())
}

pub fn process_method_parameters(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    if indexer.debug_mode {
        log_method_parameters(indexer, &node, context.current_method.as_deref());
    }

    // Check for each parameter child
    let child_count = node.named_child_count();
    for i in 0..child_count {
        if let Some(param) = node.named_child(i) {
            process_method_parameter(
                indexer,
                param,
                uri,
                source_code,
                context,
                context.current_method.as_deref(),
            )?;
        }
    }

    Ok(())
}

fn log_method_parameters(indexer: &RubyIndexer, node: &Node, method_name: Option<&str>) {
    if indexer.debug_mode {
        let method_info = method_name.unwrap_or("unknown method");
        info!(
            "Processing method parameters for {} at line {}:{}",
            method_info,
            node.start_position().row + 1,
            node.start_position().column + 1
        );
    }
}

fn process_method_parameter(
    indexer: &mut RubyIndexer,
    param: Node,
    uri: &Url,
    source_code: &str,
    context: &TraversalContext,
    method_name: Option<&str>,
) -> Result<(), String> {
    // Skip non-parameter nodes
    if !is_parameter_node(param) {
        return Ok(());
    }

    // Get the parameter name
    let param_name = get_indexer_node_text(indexer, param, source_code);

    // Skip if name is empty or invalid
    if param_name.trim().is_empty() {
        return Ok(());
    }

    // Create the parameter's fully qualified name
    let fqn = create_parameter_fqn(&param_name, context, method_name);

    // Create a range and location for this parameter
    let range = node_to_range(param);

    // Add references for the parameter
    add_parameter_references(indexer, &param_name, &fqn, uri, param);

    // Add parameter entry if in method context
    if let Some(method) = method_name {
        add_parameter_entry(indexer, &param_name, &fqn, method, uri, range, context)?;
    }

    Ok(())
}

fn is_parameter_node(node: Node) -> bool {
    match node.kind() {
        "identifier" | "optional_parameter" | "rest_parameter" | "keyword_parameter" => true,
        _ => false,
    }
}

fn create_parameter_fqn(
    param_name: &str,
    context: &TraversalContext,
    method_name: Option<&str>,
) -> String {
    // Create a fully qualified name for the parameter
    let current_namespace = context.current_namespace();

    if let Some(method) = method_name {
        if current_namespace.is_empty() {
            format!("{}#param:{}", method, param_name)
        } else {
            format!("{}#{}#param:{}", current_namespace, method, param_name)
        }
    } else {
        // No method context, just use parameter name
        param_name.to_string()
    }
}

fn add_parameter_references(
    indexer: &mut RubyIndexer,
    param_name: &str,
    fqn: &str,
    uri: &Url,
    param: Node,
) {
    // Add references to help with navigation and symbol search
    add_reference(indexer, param_name, uri, param);
    add_reference(indexer, fqn, uri, param);
}

fn add_parameter_entry(
    indexer: &mut RubyIndexer,
    param_name: &str,
    fqn: &str,
    method_name: &str,
    uri: &Url,
    range: lsp_types::Range,
    context: &TraversalContext,
) -> Result<(), String> {
    // Create and add an entry for this parameter
    let entry = EntryBuilder::new(param_name)
        .fully_qualified_name(fqn)
        .location(Location {
            uri: uri.clone(),
            range,
        })
        .entry_type(EntryType::LocalVariable)
        .visibility(context.visibility)
        .metadata("kind", "parameter")
        .metadata("container", method_name)
        .build()
        .map_err(|e| e.to_string())?;

    indexer.index.add_entry(entry);
    Ok(())
}

pub fn process_block_parameters(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    if indexer.debug_mode {
        log_block_parameters(indexer, &node, context.current_method.as_deref());
    }

    // Process each block parameter
    let child_count = node.named_child_count();
    for i in 0..child_count {
        if let Some(param) = node.named_child(i) {
            process_block_parameter(
                indexer,
                param,
                uri,
                source_code,
                context,
                context.current_method.as_deref(),
            )?;
        }
    }

    Ok(())
}

fn log_block_parameters(indexer: &RubyIndexer, node: &Node, method_name: Option<&str>) {
    if indexer.debug_mode {
        let context_info = method_name.unwrap_or("unknown block");
        info!(
            "Processing block parameters in context {} at line {}:{}",
            context_info,
            node.start_position().row + 1,
            node.start_position().column + 1
        );
    }
}

fn process_block_parameter(
    indexer: &mut RubyIndexer,
    param: Node,
    uri: &Url,
    source_code: &str,
    context: &TraversalContext,
    method_name: Option<&str>,
) -> Result<(), String> {
    // Skip non-parameter nodes
    if !is_parameter_node(param) {
        return Ok(());
    }

    // Get the parameter name
    let param_name = get_indexer_node_text(indexer, param, source_code);

    // Skip if name is empty or invalid
    if param_name.trim().is_empty() {
        return Ok(());
    }

    // Create a block scoped parameter name
    let fqn = create_block_parameter_fqn(&param_name, context, method_name);

    // Create a range for this parameter
    let range = node_to_range(param);

    // Add references for the parameter
    add_parameter_references(indexer, &param_name, &fqn, uri, param);

    Ok(())
}

fn create_block_parameter_fqn(
    param_name: &str,
    context: &TraversalContext,
    method_name: Option<&str>,
) -> String {
    // Create a fully qualified name for a block parameter
    let current_namespace = context.current_namespace();

    if let Some(method) = method_name {
        if current_namespace.is_empty() {
            format!("{}#block#param:{}", method, param_name)
        } else {
            format!(
                "{}#{}#block#param:{}",
                current_namespace, method, param_name
            )
        }
    } else {
        // No method context, just use parameter name with block prefix
        format!("block#param:{}", param_name)
    }
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
    fn test_method_parameters() {
        // Create a new indexer
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        indexer.set_debug_mode(true);

        // Define test data with method parameters
        let ruby_code = r##"
class Calculator
  def add(a, b, c = 0, *args, **kwargs)
    result = a + b + c
    args.each { |arg| result += arg }
    kwargs.each { |key, value| result += value }
    result
  end
end
"##;

        // Create a temporary file
        let (file, uri) = create_temp_ruby_file(ruby_code);

        // Index the file
        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Check if parameter entries were created
        let parameter_found = indexer.index().references.keys().any(|k| k == "a");
        assert!(parameter_found, "Parameter 'a' should be indexed");

        // Clean up
        drop(file);
    }

    #[test]
    fn test_block_parameters() {
        // Create a new indexer
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        indexer.set_debug_mode(true);

        // Define test data with block parameters
        let ruby_code = r##"
class ArrayProcessor
  def process(items)
    items.map { |item| transform(item) }
    items.each_with_index do |item, index|
      puts "Item #{index}: #{item}"
    end
  end

  def transform(item)
    item.to_s.upcase
  end
end
"##;

        // Create a temporary file
        let (file, uri) = create_temp_ruby_file(ruby_code);

        // Index the file
        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Verify params were processed
        let references = indexer.index().find_references("item");
        assert!(!references.is_empty(), "Should have references to 'item'");

        // Clean up
        drop(file);
    }
}
