use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};
use lsp_types::{Location, Url};
use tree_sitter::Node;

use super::{
    utils::{get_indexer_node_text, node_to_range},
    TraversalContext,
};

pub fn process(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Process block parameters if they exist
    if let Some(parameters) = node.child_by_field_name("parameters") {
        super::parameter_node::process_block_parameters(
            indexer,
            parameters,
            uri,
            source_code,
            context,
        )?;
    }

    // Process block body contents recursively
    process_block_body(indexer, node, uri, source_code, context)?;

    Ok(())
}

fn process_block_body(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    if let Some(body) = node.child_by_field_name("body") {
        // If there's an explicit body field, traverse its children
        traverse_children(indexer, body, uri, source_code, context)?;
    } else {
        // If there's no explicit body field, traverse all children except parameters
        traverse_non_parameter_children(indexer, node, uri, source_code, context)?;
    }

    Ok(())
}

fn traverse_children(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            indexer.traverse_node(child, uri, source_code, context)?;
        }
    }

    Ok(())
}

fn traverse_non_parameter_children(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            if child.kind() != "parameters" {
                // Skip parameters as we already processed them
                indexer.traverse_node(child, uri, source_code, context)?;
            }
        }
    }

    Ok(())
}

fn process_single_block_parameter(
    indexer: &mut RubyIndexer,
    param: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Extract parameter name
    let param_name = get_indexer_node_text(indexer, param, source_code);

    // Skip if name is empty
    if param_name.trim().is_empty() {
        return Ok(());
    }

    // Create a prefix for the block variable
    let prefix = create_block_variable_prefix(context);

    // Create a fully qualified name for this parameter
    let fqn = format!("{}${}", prefix, param_name);

    // Create a range for the parameter
    let range = node_to_range(param);

    // Create and add the entry
    add_parameter_entry(indexer, &param_name, &fqn, uri, range, context)?;

    Ok(())
}

fn create_block_variable_prefix(context: &TraversalContext) -> String {
    // Create a prefix for block variables
    let current_namespace = context.current_namespace();
    let current_method = context.current_method.as_deref().unwrap_or("block");

    if current_namespace.is_empty() {
        format!("{}#block", current_method)
    } else {
        format!("{}#{}#block", current_namespace, current_method)
    }
}

fn add_parameter_entry(
    indexer: &mut RubyIndexer,
    param_name: &str,
    fqn: &str,
    uri: &Url,
    range: lsp_types::Range,
    context: &TraversalContext,
) -> Result<(), String> {
    // Create and add an entry for a block parameter
    let entry = EntryBuilder::new(param_name)
        .fully_qualified_name(fqn)
        .location(Location {
            uri: uri.clone(),
            range,
        })
        .entry_type(EntryType::LocalVariable)
        .visibility(context.visibility)
        .metadata("kind", "block_parameter")
        .build()
        .map_err(|e| e.to_string())?;

    indexer.index.add_entry(entry);
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
    fn test_block_processing() {
        // Create a new indexer
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");

        // Test with a block that has parameters
        let ruby_code = r#"
def process_array(items)
  items.each_with_index do |item, index|
    puts "Item at index #{index}: #{item}"
  end

  items.map { |item| item.upcase }
end
"#;

        // Create a temporary file
        let (file, uri) = create_temp_ruby_file(ruby_code);

        // Index the file
        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Verify that the block parameters are indexed
        let references = indexer.index().find_references("item");
        assert!(
            !references.is_empty(),
            "Should have indexed the block parameter 'item'"
        );

        // Clean up
        drop(file);
    }
}
