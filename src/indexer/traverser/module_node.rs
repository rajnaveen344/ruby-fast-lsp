use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};
use lsp_types::{Location, Url};
use tree_sitter::Node;

use super::{
    utils::{add_reference, create_location, get_fqn, get_indexer_node_text, node_to_range},
    TraversalContext,
};

pub fn process(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Extract the module name
    let name_node = extract_module_name(node)?;
    let module_name = get_indexer_node_text(indexer, name_node, source_code);

    // Get namespace information
    let current_namespace = context.current_namespace();
    let module_fqn = get_fqn(&current_namespace, &module_name);

    // Add references
    add_module_references(indexer, &module_name, &module_fqn, uri, name_node);

    // Create and add the module entry
    let entry = create_module_entry(&module_name, &module_fqn, uri, node)?;
    indexer.index.add_entry(entry);

    // Update namespace tree
    update_namespace_tree(indexer, context, &module_fqn);

    // Process module body
    process_module_body(indexer, node, uri, source_code, context, module_name)?;

    Ok(())
}

fn extract_module_name(node: Node) -> Result<Node, String> {
    match node.child_by_field_name("name") {
        Some(node) => Ok(node),
        None => Err("Module without a name".to_string()),
    }
}

fn add_module_references(
    indexer: &mut RubyIndexer,
    module_name: &str,
    module_fqn: &str,
    uri: &Url,
    name_node: Node,
) {
    // Add reference to the module name
    add_reference(indexer, module_name, uri, name_node);

    // Also add a reference to the fully qualified name if different
    if module_name != module_fqn {
        let location = create_location(uri, name_node);
        indexer.index.add_reference(module_fqn, location);
    }
}

fn create_module_entry(
    module_name: &str,
    module_fqn: &str,
    uri: &Url,
    node: Node,
) -> Result<crate::indexer::entry::Entry, String> {
    let range = node_to_range(node);
    EntryBuilder::new(module_name)
        .fully_qualified_name(module_fqn)
        .location(Location {
            uri: uri.clone(),
            range,
        })
        .entry_type(EntryType::Module)
        .build()
        .map_err(|e| e.to_string())
}

fn update_namespace_tree(indexer: &mut RubyIndexer, context: &TraversalContext, module_fqn: &str) {
    // Determine parent namespace
    let parent_namespace = if context.namespace_stack.is_empty() {
        String::new()
    } else {
        context.current_namespace()
    };

    // Add this module to its parent's children
    let children = indexer
        .index
        .namespace_tree
        .entry(parent_namespace)
        .or_insert_with(Vec::new);

    if !children.contains(&module_fqn.to_string()) {
        children.push(module_fqn.to_string());
    }
}

fn process_module_body(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
    module_name: String,
) -> Result<(), String> {
    // Push module to namespace stack
    context.namespace_stack.push(module_name);

    // Process module body
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.named_child_count() {
            if let Some(child) = body.named_child(i) {
                indexer.traverse_node(child, uri, source_code, context)?;
            }
        }
    }

    // Pop module from namespace stack
    context.namespace_stack.pop();

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
    fn test_index_module() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r#"
        module Utils
          def self.format_name(name)
            name.capitalize
          end

          def self.version
            "1.0.0"
          end
        end
        "#;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri, ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        let index = indexer.index();

        // Verify module was indexed
        let utils_entries = index.entries.get("Utils");
        assert!(utils_entries.is_some(), "Utils module should be indexed");
        assert_eq!(
            utils_entries.unwrap()[0].entry_type,
            EntryType::Module,
            "Utils should be indexed as a module"
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

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri, ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        let index = indexer.index();

        // Verify module was indexed
        let utils_entries = index.entries.get("Utils");
        assert!(utils_entries.is_some(), "Utils module should be indexed");
        assert_eq!(
            utils_entries.unwrap()[0].entry_type,
            EntryType::Module,
            "Utils should be indexed as a module"
        );

        // Verify nested class was indexed
        let helper_entries = index.entries.get("Utils::Helper");
        assert!(
            helper_entries.is_some(),
            "Utils::Helper class should be indexed"
        );
        assert_eq!(
            helper_entries.unwrap()[0].entry_type,
            EntryType::Class,
            "Helper should be indexed as a class"
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
}
