//! FakeEditor - a stateful editor simulation for lifecycle testing.
//!
//! FakeEditor routes all operations through the real LSP handlers
//! (`handle_did_open`, `handle_did_change`, `handle_did_close`, `handle_did_save`),
//! ensuring tests exercise the exact same code paths as a real editor.
//!
//! # Tag-based assertions (simple feature tests)
//!
//! ```ignore
//! let mut editor = FakeEditor::new().await;
//! editor.open("foo.rb", "class Foo\n  def greet; end\nend").await;
//! editor.check("foo.rb", r#"
//! class Foo
//!   def $0greet; end
//! end
//! "#).await;
//! ```
//!
//! # Programmatic assertions (complex scenarios)
//!
//! ```ignore
//! let mut editor = FakeEditor::new().await;
//! editor.open("test.rb", "user = User.new\nuser.").await;
//!
//! // Type "na" after the dot
//! editor.type_at("test.rb", 1, 5, "na").await;
//!
//! // Check completions filter to "name"
//! let items = editor.complete_with_trigger("test.rb", 1, 7, ".").await;
//! assert!(items.iter().any(|i| i.label == "name"));
//!
//! // Backspace and retype
//! editor.backspace_at("test.rb", 1, 7, 2).await;
//! editor.type_at("test.rb", 1, 5, "to").await;
//! let items = editor.complete_with_trigger("test.rb", 1, 7, ".").await;
//! assert!(items.iter().any(|i| i.label == "to_s"));
//! ```
//!
//! # Available methods
//!
//! **Lifecycle**: `open`, `set`, `save`, `close`
//! **Editing**: `type_at`, `backspace_at`
//! **Queries**: `complete_at`, `complete_with_trigger`, `hover_at`, `goto_def_at`,
//!             `references_at`, `inlay_hints`, `code_lens`, `diagnostics`, `rename_at`
//! **Apply**: `apply_edit` (applies WorkspaceEdit from rename/code actions)
//! **Assertions**: `check` (tag-based), `content` (get current file content)

mod assertions;
mod documents;
mod requests;

use std::collections::HashMap;

use tower_lsp::lsp_types::{Diagnostic, InitializeParams, Url};

use crate::server::RubyLanguageServer;

/// A stateful editor simulation for testing LSP lifecycle scenarios.
///
/// Routes all operations through the real LSP handlers, ensuring tests
/// exercise the exact same code paths as a real editor. Tracks open files
/// with their content and version numbers for assertion verification.
pub struct FakeEditor {
    server: RubyLanguageServer,
    client_messages: super::client_messages::ClientMessages,
    /// Tracks open files: filename -> (clean_content, version)
    buffers: HashMap<String, (String, i32)>,
}

impl FakeEditor {
    /// Create a new FakeEditor with a fresh, initialized server.
    pub async fn new() -> Self {
        Self::with_cache_root(
            crate::utils::cache::ruby_fast_lsp_user_cache_root()
                .expect("resolve editor cache root"),
        )
        .await
    }

    pub async fn with_cache_root(root: std::path::PathBuf) -> Self {
        use tower::{Service, ServiceExt};
        let (mut service, socket) = tower_lsp::LspService::new(|client| {
            RubyLanguageServer::with_cache_root(Some(client), root)
                .expect("construct editor server")
        });
        let client_messages = super::client_messages::ClientMessages::listen(socket);
        let initialized = service
            .ready()
            .await
            .expect("LSP service ready")
            .call(
                tower_lsp::jsonrpc::Request::build("initialize")
                    .params(
                        serde_json::to_value(InitializeParams::default())
                            .expect("serialize initialize"),
                    )
                    .id(1)
                    .finish(),
            )
            .await
            .expect("initialize request succeeds")
            .expect("initialize returns a response");
        assert!(
            initialized.error().is_none(),
            "initialize failed: {initialized:?}"
        );
        Self {
            server: service.inner().clone(),
            client_messages,
            buffers: HashMap::new(),
        }
    }

    /// Choose the production resource policy before starting or sharing work.
    pub fn set_indexing_resource_policy(
        &mut self,
        policy: crate::utils::admission::IndexingResourcePolicy,
    ) {
        assert!(
            self.buffers.is_empty() && self.workspace_count() == 0,
            "configure editor resources before opening documents or adding workspaces"
        );
        self.server.set_indexing_resource_policy(policy);
    }

    /// Count actual serialized diagnostic notifications observed from the client.
    pub fn delivered_diagnostic_notifications(&self) -> u64 {
        self.client_messages.notification_count()
    }

    /// Register a workspace folder with the underlying server.
    ///
    /// Use this to test multi-root behavior. Files opened with paths under
    /// `root` (e.g. `add_workspace("workspace_a"); open("workspace_a/foo.rb", ..)`)
    /// will route to that workspace's index instead of the orphan index.
    pub fn add_workspace(&self, root: &str) {
        let uri = super::fixture_uri(format!("{}/", root.trim_end_matches('/')));
        self.server.add_workspace(uri);
    }

    /// Remove a previously added workspace folder.
    pub fn remove_workspace(&self, root: &str) {
        let uri = super::fixture_uri(format!("{}/", root.trim_end_matches('/')));
        self.server.remove_workspace(&uri);
    }

    /// Number of registered workspaces (excluding orphan).
    pub fn workspace_count(&self) -> usize {
        self.server.list_workspaces().len()
    }

    /// Look up the workspace handle (engine, root, indexing state) for a file path.
    pub fn workspace_for(&self, filename: &str) -> Option<crate::server::Workspace> {
        let uri = Self::filename_to_uri(filename);
        self.server.workspace_for_uri(&uri)
    }

    /// Get a reference to the underlying server.
    pub fn server(&self) -> &RubyLanguageServer {
        &self.server
    }

    pub fn published_diagnostics(&self, filename: &str) -> Vec<Diagnostic> {
        let uri = Self::filename_to_uri(filename);
        self.server.last_published_diagnostics(&uri)
    }

    // ─── Internal Helpers ────────────────────────────────────────────

    /// Convert a filename to a virtual URI.
    pub(super) fn filename_to_uri(filename: &str) -> Url {
        super::fixture_uri(filename)
    }

    /// Assert a file is open, panicking with a clear message if not.
    fn assert_open(&self, filename: &str, method: &str) {
        invariant!(
            self.buffers.contains_key(filename),
            what = "file '{}' is not open",
            why = "FakeEditor requests need an open buffer",
            fix = "call open() before {}()",
            filename,
            method,
        );
    }

    /// Convert a 0-indexed (line, character) position to a byte offset in content.
    fn position_to_byte_offset(content: &str, line: u32, character: u32) -> usize {
        let mut offset = 0;
        for (i, line_str) in content.split('\n').enumerate() {
            if i == line as usize {
                let target = character as usize;
                let mut utf16_units = 0;
                let mut byte_offset = 0;
                for c in line_str.chars() {
                    let char_units = c.len_utf16();
                    if utf16_units + char_units > target {
                        break;
                    }
                    utf16_units += char_units;
                    byte_offset += c.len_utf8();
                }
                return offset + byte_offset;
            }
            offset += line_str.len() + 1; // +1 for '\n'
        }
        content.len()
    }
}

/// A valid empty response is observable; a failed request must never masquerade
/// as an empty result that could satisfy a negative assertion.
fn observe_response<T>(method: &str, response: tower_lsp::jsonrpc::Result<T>) -> T {
    response.unwrap_or_else(|error| {
        unreachable_invariant!(
            what = "FakeEditor request `{method}` failed: {error:?}",
            why = "a failed request cannot satisfy an observation",
            fix = "repair the request or explicitly test its error result through the server API",
            method = method,
            error = error,
        )
    })
}

#[cfg(test)]
mod response_controls {
    use super::observe_response;

    #[test]
    fn empty_query_output_remains_distinct_from_a_failed_request() {
        let empty: Option<Vec<String>> = observe_response("references", Ok(None));
        assert_eq!(empty, None);
        let clear: Option<Vec<String>> = observe_response("references", Ok(Some(Vec::new())));
        assert_eq!(clear, Some(Vec::new()));
        let failure = std::panic::catch_unwind(|| {
            observe_response::<Option<Vec<String>>>(
                "references",
                Err(tower_lsp::jsonrpc::Error::internal_error()),
            )
        })
        .expect_err("a failed request must fail the observation");
        let message = failure
            .downcast_ref::<String>()
            .expect("control panic must be a string");
        assert!(
            message.contains("FakeEditor request `references` failed"),
            "unexpected failure class: {message}"
        );
        assert!(
            message.contains("InternalError"),
            "the original error must remain visible: {message}"
        );
    }
}
