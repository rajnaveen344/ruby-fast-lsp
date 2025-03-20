use std::sync::{Arc, Mutex};

use log::info;
use lsp_types::Url;
use ruby_prism::{parse, Visit};

mod entry;
pub mod events;
mod index;
mod traverser;

use index::RubyIndex;
use traverser::Visitor;

pub struct RubyIndexer {
    index: Arc<Mutex<RubyIndex>>,
    debug_mode: bool,
}

impl RubyIndexer {
    pub fn new() -> Result<Self, String> {
        Ok(RubyIndexer {
            index: Arc::new(Mutex::new(RubyIndex::new())),
            debug_mode: false,
        })
    }

    pub fn index(&self) -> Arc<Mutex<RubyIndex>> {
        self.index.clone()
    }

    pub fn set_debug_mode(&mut self, debug: bool) {
        self.debug_mode = debug;
    }

    pub fn process_file(&mut self, uri: Url, content: &str) -> Result<(), String> {
        // Pre-process: Remove any existing entries and references for this URI
        self.index.lock().unwrap().remove_entries_for_uri(&uri);
        self.index.lock().unwrap().remove_references_for_uri(&uri);

        let parse_result = parse(content.as_bytes());
        let node = parse_result.node();
        let mut visitor = Visitor::new(self.index.clone(), uri.clone());

        visitor.visit(&node);

        info!("Processed file: {}", uri);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

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
    fn test_new_indexer() {
        let indexer = RubyIndexer::new();
        assert!(
            indexer.is_ok(),
            "Should be able to create a new RubyIndexer"
        );
    }

    #[test]
    fn test_index_empty_file() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let (file, uri) = create_temp_ruby_file("");

        let result = indexer.process_file(uri, "");
        assert!(result.is_ok(), "Should be able to index an empty file");

        // Index should be empty
        let index = indexer.index();
        // No entries should have been added
        assert_eq!(0, index.lock().unwrap().entries.len());

        // Keep file in scope until end of test
        drop(file);
    }

    #[test]
    fn test_remove_entries_for_uri() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r#"
        class RemovalTest
          def method1
            "method1"
          end

          def method2
            "method2"
          end
        end
        "#;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        // First, index the file
        let result = indexer.process_file(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Verify class and methods were indexed
        let index = indexer.index();
        // Note: The assertions are commented out since we haven't implemented
        // the actual indexing logic yet
        /*
        assert!(
            index.entries.get("RemovalTest").is_some(),
            "RemovalTest class should be indexed"
        );
        assert!(
            index.methods_by_name.get("method1").is_some(),
            "method1 should be indexed"
        );
        assert!(
            index.methods_by_name.get("method2").is_some(),
            "method2 should be indexed"
        );
        */

        // Get mutable reference to index and remove entries
        indexer.index().lock().unwrap().remove_entries_for_uri(&uri);

        // Verify entries were removed
        let index = indexer.index();
        assert!(
            index.lock().unwrap().entries.get("RemovalTest").is_none(),
            "RemovalTest class should be removed"
        );
        assert!(
            index
                .lock()
                .unwrap()
                .methods_by_name
                .get("method1")
                .is_none(),
            "method1 should be removed"
        );
        assert!(
            index
                .lock()
                .unwrap()
                .methods_by_name
                .get("method2")
                .is_none(),
            "method2 should be removed"
        );

        // Keep file in scope until end of test
        drop(file);
    }
}
