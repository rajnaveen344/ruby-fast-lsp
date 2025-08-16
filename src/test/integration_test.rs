//! Integration test harness – see `docs/integration_test_plan.md`
//!
//! This file purposely **replaces** the previous ad-hoc integration tests with a reusable
//! and extensible test harness.  The harness makes it straightforward to add new fixtures
//! and coverage as described in the integration test plan.

use std::path::{Path, PathBuf};
use std::sync::Once;

use serde_json::Value;
use tower_lsp::lsp_types::{DidOpenTextDocumentParams, InitializeParams, TextDocumentItem, Url};
use tower_lsp::LanguageServer;

use crate::server::RubyLanguageServer;

/*----------------------------------------------------------------------
 Logger
----------------------------------------------------------------------*/

fn init_logger() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // Respect RUST_LOG env var but default to info for the test binary.
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug"))
            .is_test(true)
            .init();
    });
}

/*----------------------------------------------------------------------
 Fixture helpers
----------------------------------------------------------------------*/

/// Absolute path to the root `fixtures/` directory.
fn fixture_root() -> PathBuf {
    // `CARGO_MANIFEST_DIR` resolves to the crate root at compile time.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("fixtures")
}

/// Convert a [`Path`] to an [`Url`].  Panics if the conversion fails.
fn path_to_uri(path: &Path) -> Url {
    Url::from_file_path(path).expect("Failed to convert path to file:// URI")
}

/*----------------------------------------------------------------------
 TestHarness
----------------------------------------------------------------------*/

/// Encapsulates a `RubyLanguageServer` instance and helper utilities for
/// loading fixtures and performing common LSP requests.
pub struct TestHarness {
    server: RubyLanguageServer,
}

impl TestHarness {
    /// Create a fresh `RubyLanguageServer` instance and perform the
    /// `initialize` handshake.
    pub async fn new() -> Self {
        init_logger();

        let server = RubyLanguageServer::default();
        let _ = server.initialize(InitializeParams::default()).await;

        Self { server }
    }

    /// Opens `*.rb` files located under `fixtures/<scenario>` so that the
    /// server indexes them as a workspace.
    ///
    /// If `scenario` refers to a single Ruby file (e.g. `some_dir/foo.rb`) that
    /// file alone is opened instead of scanning a directory. This makes it
    /// possible to test language-server behaviour when only one document is
    /// open.
    pub async fn open_fixture_dir(&self, scenario: &str) {
        let root_path = fixture_root().join(scenario);
        assert!(root_path.exists(), "Unknown fixture scenario: {}", scenario);

        // -------------------------------------------------------------
        // Single-file mode – open the requested file and return early
        // -------------------------------------------------------------
        if root_path.is_file() {
            assert!(
                root_path.extension().and_then(|ext| ext.to_str()) == Some("rb"),
                "Expected a .rb file"
            );

            let uri = path_to_uri(&root_path);
            let contents = std::fs::read_to_string(&root_path)
                .unwrap_or_else(|_| panic!("Failed to read {:?}", root_path));

            let params = DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri,
                    language_id: "ruby".into(),
                    version: 1,
                    text: contents,
                },
            };

            // Fire-and-await so the document is fully indexed before continuing.
            self.server.did_open(params).await;
            return;
        }

        // -------------------------------------------------------------
        // Directory mode – recursively open every Ruby file we find
        // -------------------------------------------------------------
        let mut stack = vec![root_path];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read_dir failed") {
                let entry = entry.expect("DirEntry failed");
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|ext| ext.to_str()) == Some("rb") {
                    let uri = path_to_uri(&path);
                    let contents = std::fs::read_to_string(&path)
                        .unwrap_or_else(|_| panic!("Failed to read {:?}", path));

                    let params = DidOpenTextDocumentParams {
                        text_document: TextDocumentItem {
                            uri,
                            language_id: "ruby".into(),
                            version: 1,
                            text: contents,
                        },
                    };

                    // Fire-and-await so the document is fully indexed before continuing.
                    self.server.did_open(params).await;
                }
            }
        }
    }

    /// Borrow the underlying server – useful when calling handlers directly.
    pub fn server(&self) -> &RubyLanguageServer {
        &self.server
    }
}

/*----------------------------------------------------------------------
 Snapshot helpers
----------------------------------------------------------------------*/

/// Capture the reference locations at (`file`, `line`, `char`) and snapshot
/// the JSON array so it is easy to review when behaviour changes.
pub async fn snapshot_references(
    harness: &TestHarness,
    file: &str,
    line: u32,
    character: u32,
    snapshot_name: &str,
) {
    use crate::handlers::request;
    use tower_lsp::lsp_types::{
        PartialResultParams, Position, ReferenceContext, ReferenceParams, TextDocumentIdentifier,
        TextDocumentPositionParams, WorkDoneProgressParams,
    };

    let uri = path_to_uri(&fixture_root().join(file));
    let res_opt = request::handle_references(
        harness.server(),
        ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position { line, character },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: ReferenceContext {
                include_declaration: true,
            },
        },
    )
    .await
    .expect("goto references failed");

    let mut value = match res_opt {
        Some(locations) => serde_json::to_value(&locations).unwrap(),
        None => serde_json::json!([]),
    };

    let project_root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    relativize_uris(&mut value, &project_root);

    insta::assert_json_snapshot!(snapshot_name, value);
}

/// Capture the definition locations at (`file`, `line`, `char`) and snapshot
/// the JSON array so it is easy to review when behaviour changes.
pub async fn snapshot_definitions(
    harness: &TestHarness,
    file: &str,
    line: u32,
    character: u32,
    snapshot_name: &str,
) {
    use crate::handlers::request;
    use tower_lsp::lsp_types::{
        GotoDefinitionParams, PartialResultParams, Position, TextDocumentIdentifier,
        TextDocumentPositionParams, WorkDoneProgressParams,
    };

    let uri = path_to_uri(&fixture_root().join(file));
    let res_opt = request::handle_goto_definition(
        harness.server(),
        GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position { line, character },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await
    .expect("goto definition failed");

    // Convert the LSP response (if any) into a JSON value so it can be snapshotted.
    // If there is **no** definition then we snapshot an empty JSON array so callers can
    // assert on the absence of definitions without causing a test failure.
    let mut value = match res_opt {
        Some(tower_lsp::lsp_types::GotoDefinitionResponse::Array(loc)) => {
            serde_json::to_value(&loc).unwrap()
        }
        Some(tower_lsp::lsp_types::GotoDefinitionResponse::Scalar(l)) => {
            serde_json::to_value(&vec![l]).unwrap()
        }
        Some(tower_lsp::lsp_types::GotoDefinitionResponse::Link(ls)) => {
            serde_json::to_value(&ls).unwrap()
        }
        None => serde_json::json!([]),
    };

    let project_root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    relativize_uris(&mut value, &project_root);

    insta::assert_json_snapshot!(snapshot_name, value);
}

/// Replace absolute `file://` URIs inside `value` with a `$PROJECT_ROOT` placeholder so
/// that Insta snapshots are stable across machines.
///
/// * `value` – JSON value that will be mutated in-place.
/// * `project_root` – absolute path to the crate root (usually `env!("CARGO_MANIFEST_DIR")`).
fn relativize_uris(value: &mut Value, project_root: &Path) {
    // Build a canonical `file://` prefix using forward slashes so the check works on all OSes.
    let mut prefix = String::from("file://");
    prefix.push_str(&project_root.display().to_string().replace('\\', "/"));
    if !prefix.ends_with('/') {
        prefix.push('/');
    }

    // Recursive helper – kept private to the function.
    fn walk(v: &mut Value, prefix: &str) {
        match v {
            Value::String(s) if s.starts_with(prefix) => {
                let rel = &s[prefix.len()..];
                *s = format!("file://$PROJECT_ROOT/{}", rel);
            }
            Value::Array(arr) => arr.iter_mut().for_each(|child| walk(child, prefix)),
            Value::Object(map) => map.values_mut().for_each(|child| walk(child, prefix)),
            _ => {}
        }
    }

    walk(value, &prefix);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer_prism::visitors::index_visitor::IndexVisitor;
    use crate::indexer::dependency_tracker::DependencyTracker;
    use crate::server::RubyLanguageServer;
    use crate::types::ruby_document::RubyDocument;
    use parking_lot::{Mutex, RwLock};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tower_lsp::lsp_types::Url;

    /// Simply verifies that we can load a fixture directory without panicking.
    #[tokio::test]
    async fn harness_smoke() {
        let harness = TestHarness::new().await;
        harness.open_fixture_dir("goto").await;
        assert!(true);
    }

    #[tokio::test]
    async fn test_dependency_tracking_integration() {
        init_logger();

        // Create a temporary directory for test files
        let temp_dir = tempfile::TempDir::new().unwrap();
        let temp_path = temp_dir.path();
        
        // Create the lib directory and set.rb file
        let lib_dir = temp_path.join("lib");
        std::fs::create_dir_all(&lib_dir).unwrap();
        let set_file = lib_dir.join("set.rb");
        std::fs::write(&set_file, "class Set\n  # Set implementation\nend\n").unwrap();

        // Create a mock server
        let server = RubyLanguageServer::default();

        // Create dependency tracker
        let lib_dirs = vec![lib_dir.clone()];
        let dependency_tracker = Arc::new(Mutex::new(DependencyTracker::new(
            temp_path.to_path_buf(),
            lib_dirs,
        )));

        // Create test file URL
        let test_file = temp_path.join("test_set.rb");
        let uri = Url::from_file_path(&test_file).unwrap();

        // Test content with require_relative
        let content = r#"
require_relative 'lib/set'

class TestClass
  def initialize
    @my_set = Set.new
  end
end
"#;

        // Write the test file to disk
        std::fs::write(&test_file, content).unwrap();

        // Insert a RubyDocument so that IndexVisitor::new can retrieve it
        let doc = RubyDocument::new(uri.clone(), content.to_string(), 0);
        server
            .docs
            .lock()
            .insert(uri.clone(), Arc::new(RwLock::new(doc.clone())));

        // Create IndexVisitor with dependency tracker
        let mut visitor = IndexVisitor::new(&server, uri.clone())
            .with_dependency_tracker(dependency_tracker.clone());

        // Parse and visit the content
        let parse_result = ruby_prism::parse(content.as_bytes());
        let root = parse_result.node();

        use ruby_prism::Visit;
        visitor.visit(&root);

        // Check that the dependency was tracked
        let tracker_guard = dependency_tracker.lock();
        let stats = tracker_guard.get_stats();

        println!("Dependency stats: {:?}", stats);
        assert!(
            stats.total_dependencies > 0,
            "Should have tracked at least one dependency"
        );
    }
}
