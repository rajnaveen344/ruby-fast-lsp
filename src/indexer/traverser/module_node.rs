use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};
use log::info;
use lsp_types::{Location, Url};
use tree_sitter::Node;

use super::{utils::node_to_range, TraversalContext};

pub fn process(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Find the module name node
    let name_node = match node.child_by_field_name("name") {
        Some(node) => node,
        None => {
            // Skip anonymous or dynamically defined modules instead of failing
            if indexer.debug_mode {
                info!(
                    "Skipping module without a name at {}:{}",
                    node.start_position().row + 1,
                    node.start_position().column + 1
                );
            }

            // Still traverse children for any defined methods or constants
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    indexer.traverse_node(child, uri, source_code, context)?;
                }
            }

            return Ok(());
        }
    };

    // Extract the name text
    let name = indexer.get_node_text(name_node, source_code);

    // Skip modules with empty names or just whitespace
    if name.trim().is_empty() {
        if indexer.debug_mode {
            info!(
                "Skipping module with empty name at {}:{}",
                node.start_position().row + 1,
                node.start_position().column + 1
            );
        }

        // Still traverse children
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                indexer.traverse_node(child, uri, source_code, context)?;
            }
        }

        return Ok(());
    }

    // Create a fully qualified name by joining the namespace stack
    let current_namespace = context.current_namespace();

    let fqn = if current_namespace.is_empty() {
        name.clone()
    } else {
        format!("{}::{}", current_namespace, name)
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
        .entry_type(EntryType::Module)
        .build()?;

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

    if !children.contains(&name) {
        children.push(name.clone());
    }

    // Push the module name onto the namespace stack
    context.namespace_stack.push(name);

    // Process the body of the module
    if let Some(body_node) = node.child_by_field_name("body") {
        indexer.traverse_node(body_node, uri, source_code, context)?;
    }

    // Pop the namespace when done
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
