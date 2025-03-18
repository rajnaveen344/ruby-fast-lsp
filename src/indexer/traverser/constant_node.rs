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
    let constant = node;
    let parent = constant.parent();

    // Process constants based on their context
    if let Some(parent_node) = parent {
        match parent_node.kind() {
            "assignment" => process_constant_assignment(
                indexer,
                constant,
                parent_node,
                uri,
                source_code,
                context,
            )?,
            "scope_resolution" => process_constant_scoped(indexer, constant, uri, source_code)?,
            _ => process_constant_reference(indexer, constant, uri, source_code, context)?,
        }
    } else {
        process_constant_reference(indexer, constant, uri, source_code, context)?;
    }

    // Continue traversing children
    traverse_children(indexer, node, uri, source_code, context)?;

    Ok(())
}

fn traverse_children(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            indexer.traverse_node(child, uri, source_code, context)?;
        }
    }
    Ok(())
}

pub fn process_constant_assignment(
    indexer: &mut RubyIndexer,
    constant: Node,
    parent_node: Node,
    uri: &Url,
    source_code: &str,
    context: &TraversalContext,
) -> Result<(), String> {
    // Extract constant name
    let constant_name = get_indexer_node_text(indexer, constant, source_code);

    // Create a range for the constant definition
    let range = node_to_range(constant);

    // Determine the fully qualified name
    let fully_qualified_name = create_fully_qualified_name(&constant_name, context);

    // Debug logging
    log_constant_processing(indexer, &constant_name, &fully_qualified_name, constant);

    // Create and add the constant entry
    add_constant_entry(
        indexer,
        &constant_name,
        &fully_qualified_name,
        uri,
        range,
        context,
    )?;

    // Add reference to help with lookup
    add_reference(indexer, &constant_name, uri, constant);
    add_reference(indexer, &fully_qualified_name, uri, constant);

    Ok(())
}

fn create_fully_qualified_name(constant_name: &str, context: &TraversalContext) -> String {
    let current_namespace = context.current_namespace();
    if current_namespace.is_empty() {
        constant_name.to_string()
    } else {
        format!("{}::{}", current_namespace, constant_name)
    }
}

fn log_constant_processing(
    indexer: &RubyIndexer,
    constant_name: &str,
    fully_qualified_name: &str,
    constant: Node,
) {
    if indexer.debug_mode {
        info!(
            "Processing constant: {} (FQN: {}) at line {}:{}",
            constant_name,
            fully_qualified_name,
            constant.start_position().row + 1,
            constant.start_position().column + 1
        );
    }
}

fn add_constant_entry(
    indexer: &mut RubyIndexer,
    constant_name: &str,
    fully_qualified_name: &str,
    uri: &Url,
    range: lsp_types::Range,
    context: &TraversalContext,
) -> Result<(), String> {
    let constant_entry = EntryBuilder::new(constant_name)
        .fully_qualified_name(fully_qualified_name)
        .location(Location {
            uri: uri.clone(),
            range,
        })
        .entry_type(EntryType::Constant)
        .visibility(context.visibility)
        .build()
        .map_err(|e| e.to_string())?;

    indexer.index.add_entry(constant_entry);
    Ok(())
}

pub fn process_constant_scoped(
    indexer: &mut RubyIndexer,
    constant: Node,
    uri: &Url,
    source_code: &str,
) -> Result<(), String> {
    // For scope resolution (Namespace::Constant), we just add a reference to help with lookup
    let constant_text = get_indexer_node_text(indexer, constant, source_code);

    // Try to get the fully qualified name including parent namespaces
    if let Some(parent) = constant.parent() {
        if parent.kind() == "scope_resolution" {
            if let Some(fqn) = get_fully_qualified_scope(indexer, parent, source_code) {
                add_reference(indexer, &fqn, uri, parent);
            }
        }
    }

    // Also add a reference to just the constant name itself
    add_reference(indexer, &constant_text, uri, constant);

    Ok(())
}

pub fn process_constant_reference(
    indexer: &mut RubyIndexer,
    constant: Node,
    uri: &Url,
    source_code: &str,
    context: &TraversalContext,
) -> Result<(), String> {
    // For regular constant references, add both simple and qualified name references
    let constant_text = get_indexer_node_text(indexer, constant, source_code);

    // Add a reference to the constant itself
    add_reference(indexer, &constant_text, uri, constant);

    // Get parent node for possible scope resolution
    let parent = constant.parent();
    if let Some(parent_node) = parent {
        process_parent_node(
            indexer,
            parent_node,
            constant,
            &constant_text,
            uri,
            source_code,
        )?;
    } else {
        // Add qualified reference with current namespace if not in a scope resolution
        add_qualified_reference(indexer, constant, &constant_text, uri, context);
    }

    Ok(())
}

fn process_parent_node(
    indexer: &mut RubyIndexer,
    parent_node: Node,
    constant: Node,
    constant_text: &str,
    uri: &Url,
    source_code: &str,
) -> Result<(), String> {
    // Handle scope resolution (Namespace::Constant)
    if parent_node.kind() == "scope_resolution" {
        if let Some(fqn) = get_fully_qualified_scope(indexer, parent_node, source_code) {
            // Add reference to the fully qualified name
            add_reference(indexer, &fqn, uri, parent_node);

            // Also add references to each part of the nested path
            add_nested_path_references(indexer, &fqn, uri, parent_node);
        }
    }

    Ok(())
}

fn add_nested_path_references(indexer: &mut RubyIndexer, fqn: &str, uri: &Url, node: Node) {
    let parts: Vec<&str> = fqn.split("::").collect();
    if parts.len() > 1 {
        // Add references to each part of the namespace path
        for part in parts {
            let location = create_location(uri, node);
            indexer.index.add_reference(part, location.clone());
        }
    }
}

fn add_qualified_reference(
    indexer: &mut RubyIndexer,
    constant: Node,
    constant_text: &str,
    uri: &Url,
    context: &TraversalContext,
) {
    // Add a reference with the current namespace as prefix
    let current_namespace = context.current_namespace();
    if !current_namespace.is_empty() {
        let qualified_name = format!("{}::{}", current_namespace, constant_text);
        let location = create_location(uri, constant);
        indexer.index.add_reference(&qualified_name, location);
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
    fn test_constant_processing() {
        // Create a new indexer
        let mut indexer = RubyIndexer::new().unwrap();
        indexer.set_debug_mode(true);

        // Create a simple Ruby file with a constant definition
        let ruby_code = r#"
module TestModule
  CONSTANT_VALUE = 42

  class TestClass
    NESTED_CONSTANT = "test"

    def test_method
      puts NESTED_CONSTANT
      puts TestModule::CONSTANT_VALUE
      puts ::TestModule::CONSTANT_VALUE
    end
  end
end
"#;

        // Create a temporary file
        let (temp_file, uri) = create_temp_ruby_file(ruby_code);

        // Index the file
        indexer.index_file_with_uri(uri.clone(), ruby_code).unwrap();

        // Verify constants were indexed
        let constant_entries = indexer.index().constants_by_name.get("CONSTANT_VALUE");
        assert!(
            constant_entries.is_some(),
            "CONSTANT_VALUE should be indexed"
        );

        let nested_constant_entries = indexer.index().constants_by_name.get("NESTED_CONSTANT");
        assert!(
            nested_constant_entries.is_some(),
            "NESTED_CONSTANT should be indexed"
        );

        // Verify fully qualified names
        let fqn_references = indexer
            .index()
            .find_references("TestModule::CONSTANT_VALUE");
        assert!(
            !fqn_references.is_empty(),
            "Should have references to TestModule::CONSTANT_VALUE"
        );

        let nested_fqn_references = indexer
            .index()
            .find_references("TestModule::TestClass::NESTED_CONSTANT");
        assert!(
            !nested_fqn_references.is_empty(),
            "Should have references to TestModule::TestClass::NESTED_CONSTANT"
        );

        // Verify references
        let constant_references = indexer.index().find_references("CONSTANT_VALUE");
        assert!(
            !constant_references.is_empty(),
            "Should have references to CONSTANT_VALUE"
        );

        // Clean up
        drop(temp_file);
    }
}
