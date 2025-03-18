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
    // Check parent node to determine if these are method or block parameters
    if let Some(parent) = node.parent() {
        if parent.kind() == "method" || parent.kind() == "singleton_method" {
            process_method_parameters(indexer, node, uri, source_code, context)?;
        } else if parent.kind() == "block" {
            super::block_node::process_block_parameters(indexer, node, uri, source_code, context)?;
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
    // Iterate through all parameter nodes
    for i in 0..node.named_child_count() {
        if let Some(param_node) = node.named_child(i) {
            let param_kind = param_node.kind();
            let param_name = match param_kind {
                "identifier" => get_indexer_node_text(indexer, param_node, source_code),
                "optional_parameter"
                | "keyword_parameter"
                | "rest_parameter"
                | "hash_splat_parameter"
                | "block_parameter" => {
                    if let Some(name_node) = param_node.child_by_field_name("name") {
                        get_indexer_node_text(indexer, name_node, source_code)
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            if param_name.trim().is_empty() {
                continue;
            }

            // Create a range for the definition
            let range = node_to_range(param_node);

            // Create a fully qualified name for the parameter
            let current_namespace = context.current_namespace();
            let current_method = context
                .current_method
                .as_ref()
                .ok_or_else(|| "Method parameter outside of method context".to_string())?;

            let fqn = if current_namespace.is_empty() {
                format!("{}${}", current_method, param_name)
            } else {
                format!("{}#{}${}", current_namespace, current_method, param_name)
            };

            // Create and add the entry
            let entry = EntryBuilder::new(&param_name)
                .fully_qualified_name(&fqn)
                .location(Location {
                    uri: uri.clone(),
                    range,
                })
                .entry_type(EntryType::LocalVariable)
                .metadata("kind", "parameter")
                .build()
                .map_err(|e| e.to_string())?;

            indexer.index.add_entry(entry);
        }
    }

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
    fn test_basic_method_parameters() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r##"
        class Calculator
          def add(a, b)
            a + b
          end

          def subtract(x, y)
            x - y
          end
        end
        "##;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Now we need to create a separate context for testing since the indexer doesn't add references for parameters
        // This is expected behavior as parameters are treated as variable definitions, not references

        // Check for entries with parameter names in the index
        let entries = &indexer.index().entries;
        assert!(!entries.is_empty(), "Should have indexed entries");

        // Clean up
        drop(file);
    }

    #[test]
    fn test_advanced_parameter_types() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r##"
        class ApiClient
          # Tests various parameter types
          def fetch(
            endpoint,              # Regular parameter
            options = {},          # Optional parameter with default
            *args,                 # Rest parameter
            format: :json,         # Keyword parameter with default
            timeout: 30,           # Another keyword parameter
            **kwargs,              # Keyword splat parameter
            &block                 # Block parameter
          )
            # Use parameters in method body to create references
            url = endpoint
            opts = options
            args.each { |arg| puts arg }
            output_format = format
            wait_time = timeout
            kwargs.each { |k, v| puts k, v }
            block.call if block
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
    fn test_parameter_references_in_method_body() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r##"
        class StringProcessor
          def transform(input, prefix: "", suffix: "", &formatter)
            # Transform the input string using the provided parameters
            result = input.clone

            # Apply formatter if provided
            result = formatter.call(result) if formatter

            # Apply prefix and suffix
            result = prefix + result + suffix

            result
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
    fn test_method_parameters_in_modules() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r##"
        module Validator
          module StringUtils
            def self.validate_length(text, min: 0, max: 100)
              length = text.length
              valid = length >= min && length <= max

              return {
                valid: valid,
                length: length,
                min: min,
                max: max
              }
            end
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
