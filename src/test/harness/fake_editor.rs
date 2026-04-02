//! FakeEditor - a stateful editor simulation for lifecycle testing.
//!
//! FakeEditor is the single implementation path for all test assertions.
//! It owns a `RubyLanguageServer`, tracks open files with content and versions,
//! and provides methods to open, edit, close, and assert against files.
//!
//! # Usage
//!
//! ```ignore
//! let mut editor = FakeEditor::new().await;
//! editor.open("foo.rb", "class Foo; end");
//! editor.set("foo.rb", "class Foo\n  def greet; end\nend");
//! editor.check("foo.rb", r#"
//! class Foo
//!   def $0greet; end
//! end
//! "#).await;
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use tower_lsp::lsp_types::{InitializeParams, Url};
use tower_lsp::LanguageServer;

use crate::indexer::file_processor::{FileProcessor, ProcessingOptions};
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;

use super::check::{run_checks_on_fixture, strip_all_markers};

/// A stateful editor simulation for testing LSP lifecycle scenarios.
///
/// Tracks open files with their content and version numbers, provides
/// methods to simulate editor actions, and delegates assertions to the
/// shared `run_checks_on_fixture` engine.
pub struct FakeEditor {
    server: RubyLanguageServer,
    /// Tracks open files: filename -> (clean_content, version)
    buffers: HashMap<String, (String, i32)>,
}

impl FakeEditor {
    /// Create a new FakeEditor with a fresh, initialized server.
    pub async fn new() -> Self {
        let server = RubyLanguageServer::default();
        let _ = server.initialize(InitializeParams::default()).await;
        FakeEditor {
            server,
            buffers: HashMap::new(),
        }
    }

    /// Open a file in the editor with the given content.
    ///
    /// Creates a document, indexes it, and tracks it in the buffer list.
    /// Panics if the file is already open (use `set()` to update).
    pub fn open(&mut self, filename: &str, content: &str) {
        assert!(
            !self.buffers.contains_key(filename),
            "INVARIANT VIOLATED: File '{}' is already open. Use set() to update content.",
            filename
        );

        let uri = Self::filename_to_uri(filename);
        let version = 1;

        // Create document and insert into server
        let document = RubyDocument::new(uri.clone(), content.to_string(), version);
        self.server
            .docs
            .lock()
            .insert(uri.clone(), Arc::new(RwLock::new(document)));

        // Index the document
        self.index_file(&uri, content);

        // Track in buffers
        self.buffers
            .insert(filename.to_string(), (content.to_string(), version));
    }

    /// Update an open file's content.
    ///
    /// Bumps the version, updates the document, and re-indexes.
    /// Panics if the file is not open (use `open()` first).
    pub fn set(&mut self, filename: &str, new_content: &str) {
        let (_, version) = self.buffers.get(filename).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: File '{}' is not open. Call open() before set().",
                filename
            )
        });
        let new_version = version + 1;

        let uri = Self::filename_to_uri(filename);

        // Update the document
        {
            let docs = self.server.docs.lock();
            let doc_arc = docs.get(&uri).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: File '{}' is in buffers but not in server.docs. This is a bug in FakeEditor.",
                    filename
                )
            });
            let mut doc = doc_arc.write();
            doc.update(new_content.to_string(), new_version);
        }

        // Re-index
        self.index_file(&uri, new_content);

        // Update buffer tracking
        self.buffers
            .insert(filename.to_string(), (new_content.to_string(), new_version));
    }

    /// Close a file in the editor.
    ///
    /// Removes from both buffer tracking and server document cache.
    /// Index entries are preserved (matching real LSP behavior).
    pub fn close(&mut self, filename: &str) {
        assert!(
            self.buffers.remove(filename).is_some(),
            "INVARIANT VIOLATED: File '{}' is not open. Cannot close a file that was never opened.",
            filename
        );

        let uri = Self::filename_to_uri(filename);
        self.server.docs.lock().remove(&uri);
    }

    /// Run tag-based assertions against a file's current state.
    ///
    /// The `fixture` contains markers ($0, <def>, <ref>, <hint>, etc.) embedded
    /// in the expected file content. The clean content extracted from the fixture
    /// must match the file's current buffer content.
    pub async fn check(&self, filename: &str, fixture: &str) {
        let (buffer_content, _) = self.buffers.get(filename).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: File '{}' is not open. Call open() before check().",
                filename
            )
        });

        let fixture_content = strip_all_markers(fixture);

        // Verify fixture content matches current buffer (catches test authoring bugs)
        assert_eq!(
            buffer_content.trim(),
            fixture_content.trim(),
            "Fixture content doesn't match buffer for '{}'. \
             The clean content from the fixture must match what was passed to open() or set().\n\
             Buffer:\n{}\n\nFixture (cleaned):\n{}",
            filename,
            buffer_content,
            fixture_content
        );

        let uri = Self::filename_to_uri(filename);
        run_checks_on_fixture(&self.server, &uri, buffer_content, fixture, None).await;
    }

    /// Get a reference to the underlying server.
    pub fn server(&self) -> &RubyLanguageServer {
        &self.server
    }

    /// Convert a filename to a virtual URI.
    pub(super) fn filename_to_uri(filename: &str) -> Url {
        Url::parse(&format!("file:///{}", filename)).expect("Invalid virtual URI")
    }

    /// Index a file using FileProcessor.
    fn index_file(&self, uri: &Url, content: &str) {
        let indexer = FileProcessor::new(self.server.index.clone());
        let options = ProcessingOptions {
            index_definitions: true,
            index_references: true,
            resolve_mixins: true,
            include_local_vars: true,
        };
        let _ = indexer.process_file(uri, content, &self.server, options);
    }
}
