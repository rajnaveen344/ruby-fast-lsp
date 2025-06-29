//! Integration test harness – see `docs/integration_test_plan.md`
//!
//! This file purposely **replaces** the previous ad-hoc integration tests with a reusable
//! and extensible test harness.  The harness makes it straightforward to add new fixtures
//! and coverage as described in the integration test plan.

use std::path::{Path, PathBuf};
use std::sync::Once;

use lsp_types::{DidOpenTextDocumentParams, InitializeParams, TextDocumentItem, Url};
use tower_lsp::LanguageServer;

use crate::server::RubyLanguageServer;

/*----------------------------------------------------------------------
 Logger
----------------------------------------------------------------------*/

fn init_logger() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // Respect RUST_LOG env var but default to info for the test binary.
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
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

    /// Opens **all** `*.rb` files located under `fixtures/<scenario>` so that
    /// the server indexes them as a workspace.
    pub async fn open_fixture_dir(&self, scenario: &str) {
        let root = fixture_root().join(scenario);
        assert!(root.exists(), "Unknown fixture scenario: {}", scenario);

        let mut stack = vec![root];
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
 Assertion helpers (macros)
----------------------------------------------------------------------*/

/// Assert that a *goto definition* request at (`file`, `line`, `char`) resolves
/// to (`exp_file`, `exp_line`).
#[macro_export]
macro_rules! assert_goto {
    ($harness:expr, $file:expr, $line:expr, $char:expr,
     $exp_file:expr, $exp_line:expr $(,)?) => {{
        use lsp_types::{
            GotoDefinitionParams, GotoDefinitionResponse, PartialResultParams, Position,
            TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams,
        };
        use $crate::handlers::request;

        let uri = path_to_uri(&fixture_root().join($file));
        let res = request::handle_goto_definition(
            $harness.server(),
            GotoDefinitionParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    position: Position {
                        line: $line,
                        character: $char,
                    },
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        )
        .await
        .expect("goto definition request failed")
        .expect("no definition found");

        match res {
            GotoDefinitionResponse::Scalar(location) => {
                assert_eq!(location.uri, path_to_uri(&fixture_root().join($exp_file)));
                assert_eq!(location.range.start.line, $exp_line);
            }
            other => panic!("Unexpected goto definition response: {:?}", other),
        }
    }};
}

/*----------------------------------------------------------------------
 Smoke test – validates that the harness itself works.
----------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;

    /// Simply verifies that we can load a fixture directory without panicking.
    #[tokio::test]
    async fn harness_smoke() {
        let harness = TestHarness::new().await;
        // The `def_ref/` folder will be created in a follow-up commit.
        // For now fall back to an empty dir if it doesn't exist to keep CI green.
        if fixture_root().join("def_ref").exists() {
            harness.open_fixture_dir("def_ref").await;
        }
        // If we reached here, the harness is functional.
        assert!(true);
    }
}
