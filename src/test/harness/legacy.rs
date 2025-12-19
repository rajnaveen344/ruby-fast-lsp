//! Legacy TestHarness for fixture-based tests.
//!
//! This provides compatibility for tests that still use external fixture files.

use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::{DidOpenTextDocumentParams, InitializeParams, TextDocumentItem, Url};
use tower_lsp::LanguageServer;

use crate::server::RubyLanguageServer;

/// Absolute path to the root `fixtures/` directory.
pub fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/test/fixtures")
}

/// Convert a [`Path`] to an [`Url`]. Panics if the conversion fails.
pub fn path_to_uri(path: &Path) -> Url {
    Url::from_file_path(path).expect("Failed to convert path to file:// URI")
}

/// Encapsulates a `RubyLanguageServer` instance and helper utilities for
/// loading fixtures and performing common LSP requests.
pub struct TestHarness {
    server: RubyLanguageServer,
}

impl TestHarness {
    /// Create a fresh `RubyLanguageServer` instance and perform the
    /// `initialize` handshake.
    pub async fn new() -> Self {
        let server = RubyLanguageServer::default();
        server.initialize(InitializeParams::default()).await.ok();
        Self { server }
    }

    /// Opens `*.rb` files located under `fixtures/<scenario>`.
    pub async fn open_fixture_dir(&self, scenario: &str) {
        let base = fixture_root().join(scenario);

        let files: Vec<PathBuf> = if base.is_file() {
            vec![base]
        } else {
            std::fs::read_dir(&base)
                .expect("Failed to read fixture directory")
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("rb") {
                        Some(path)
                    } else {
                        None
                    }
                })
                .collect()
        };

        for file in files {
            let text = std::fs::read_to_string(&file).expect("Failed to read fixture file");
            let uri = path_to_uri(&file);

            self.server
                .did_open(DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri,
                        language_id: "ruby".into(),
                        version: 1,
                        text,
                    },
                })
                .await;
        }
    }

    /// Borrow the underlying server.
    pub fn server(&self) -> &RubyLanguageServer {
        &self.server
    }
}
