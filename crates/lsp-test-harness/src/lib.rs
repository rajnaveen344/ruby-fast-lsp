use std::collections::HashMap;

use ruby_fast_lsp::extensions::{ExtensionStatusParams, ExtensionStatusReport};
use ruby_fast_lsp::server::RubyLanguageServer;
use tower_lsp::jsonrpc::ErrorCode;
use tower_lsp::lsp_types::{
    CodeLens, CodeLensParams, CompletionContext, CompletionItem, CompletionParams,
    CompletionResponse, CompletionTriggerKind, DidChangeTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams, InitializeParams,
    InitializedParams, Location, PartialResultParams, Position, ReferenceContext, ReferenceParams,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, Url, VersionedTextDocumentIdentifier, WorkDoneProgressParams,
};
use tower_lsp::LanguageServer;

const GOTO_DEFINITION_RETRIGGER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const GOTO_DEFINITION_RETRIGGER_BACKOFF: std::time::Duration = std::time::Duration::from_millis(25);

pub struct FakeEditor {
    server: RubyLanguageServer,
    buffers: HashMap<String, (String, i32)>,
}

impl FakeEditor {
    pub async fn new() -> Self {
        Self::new_with_initialization_options(None).await
    }

    pub async fn new_with_initialization_options(
        initialization_options: Option<serde_json::Value>,
    ) -> Self {
        let server = RubyLanguageServer::default();
        server
            .initialize(InitializeParams {
                initialization_options,
                ..InitializeParams::default()
            })
            .await
            .expect("INVARIANT VIOLATED: FakeEditor failed to initialize RubyLanguageServer. This is a bug because tests require a valid LSP initialization. Fix: keep server initialization valid for default params.");
        server.initialized(InitializedParams {}).await;

        Self {
            server,
            buffers: HashMap::new(),
        }
    }

    pub async fn with_extension_package(package_path: impl AsRef<std::path::Path>) -> Self {
        Self::new_with_initialization_options(Some(serde_json::json!({
            "extensionPackages": [package_path.as_ref().to_string_lossy().to_string()],
            "extensionDirs": []
        })))
        .await
    }

    pub async fn with_extension_package_and_workspace(
        package_path: impl AsRef<std::path::Path>,
        workspace_root: impl AsRef<std::path::Path>,
    ) -> Self {
        Self::with_extension_packages_and_workspace([package_path], workspace_root).await
    }

    pub async fn with_extension_packages_and_workspace<I, P>(
        package_paths: I,
        workspace_root: impl AsRef<std::path::Path>,
    ) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<std::path::Path>,
    {
        let server = RubyLanguageServer::default();
        let root_uri = Url::from_directory_path(workspace_root.as_ref()).expect(
            "INVARIANT VIOLATED: black-box workspace root is not a valid file URI. This is a test setup bug because project-context tests require a real filesystem root. Fix: create the workspace with tempfile.",
        );
        let extension_packages = package_paths
            .into_iter()
            .map(|path| path.as_ref().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        server
            .initialize(InitializeParams {
                root_uri: Some(root_uri),
                initialization_options: Some(serde_json::json!({
                    "extensionPackages": extension_packages,
                    "extensionDirs": [],
                    "workspaceTrusted": true
                })),
                ..InitializeParams::default()
            })
            .await
            .expect("INVARIANT VIOLATED: project-aware FakeEditor failed to initialize RubyLanguageServer. This is a test harness bug because the supplied workspace and extension package are valid. Fix: inspect initialization routing.");
        server.initialized(InitializedParams {}).await;

        Self {
            server,
            buffers: HashMap::new(),
        }
    }

    pub async fn open(&mut self, filename: &str, content: &str) {
        assert!(
            !self.buffers.contains_key(filename),
            "INVARIANT VIOLATED: file `{}` is already open. \
             This is a bug because FakeEditor open must model LSP didOpen exactly once. \
             Fix: call set() for existing buffers.",
            filename
        );

        let uri = filename_to_uri(filename);
        let version = 1;
        self.server
            .did_open(DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri,
                    language_id: "ruby".to_string(),
                    version,
                    text: content.to_string(),
                },
            })
            .await;
        self.buffers
            .insert(filename.to_string(), (content.to_string(), version));
    }

    pub async fn set(&mut self, filename: &str, content: &str) {
        let (_, version) = self.buffers.get(filename).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: file `{}` is not open. \
                 This is a bug because FakeEditor set must model didChange after didOpen. \
                 Fix: call open() before set().",
                filename
            )
        });
        let new_version = *version + 1;
        let uri = filename_to_uri(filename);

        self.server
            .did_change(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri,
                    version: new_version,
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: content.to_string(),
                }],
            })
            .await;

        self.buffers
            .insert(filename.to_string(), (content.to_string(), new_version));
    }

    pub async fn document_symbols(&self, filename: &str) -> Vec<DocumentSymbol> {
        self.assert_open(filename, "document_symbols");
        let uri = filename_to_uri(filename);
        let response = self
            .server
            .document_symbol(DocumentSymbolParams {
                text_document: TextDocumentIdentifier { uri },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            })
            .await
            .expect("INVARIANT VIOLATED: document_symbol request failed. This is a bug because FakeEditor expects in-process LSP calls to return JSON-RPC success. Fix: inspect request handler error path.");

        match response {
            Some(DocumentSymbolResponse::Nested(symbols)) => symbols,
            Some(DocumentSymbolResponse::Flat(_)) => panic!(
                "INVARIANT VIOLATED: document_symbol returned flat symbols. \
                 This is a bug because Ruby Fast LSP currently returns nested document symbols. \
                 Fix: update FakeEditor if flat response becomes supported."
            ),
            None => Vec::new(),
        }
    }

    pub async fn code_lens(&self, filename: &str) -> Vec<CodeLens> {
        self.assert_open(filename, "code_lens");
        let uri = filename_to_uri(filename);
        self.server
            .code_lens(CodeLensParams {
                text_document: TextDocumentIdentifier { uri },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            })
            .await
            .expect("INVARIANT VIOLATED: code_lens request failed. This is a bug because FakeEditor expects in-process LSP calls to return JSON-RPC success. Fix: inspect request handler error path.")
            .unwrap_or_default()
    }

    pub async fn goto_definition(
        &self,
        filename: &str,
        line: u32,
        character: u32,
    ) -> Vec<Location> {
        self.assert_open(filename, "goto_definition");
        let uri = filename_to_uri(filename);
        let response = {
            let deadline = tokio::time::Instant::now() + GOTO_DEFINITION_RETRIGGER_TIMEOUT;
            let mut retriggers = 0;
            loop {
                let remaining = deadline
                    .checked_duration_since(tokio::time::Instant::now())
                    .unwrap_or_else(|| {
                        panic!(
                            "INVARIANT VIOLATED: goto_definition exceeded the bounded {:?} retrigger window after {retriggers} retriggers. This is a bug because a valid indexing generation must reach a target or terminal absence. Fix: inspect the stuck project phase and demand lifecycle.",
                            GOTO_DEFINITION_RETRIGGER_TIMEOUT
                        )
                    });
                let result = tokio::time::timeout(
                    remaining,
                    self.server.goto_definition(GotoDefinitionParams {
                        text_document_position_params: TextDocumentPositionParams {
                            text_document: TextDocumentIdentifier { uri: uri.clone() },
                            position: Position { line, character },
                        },
                        work_done_progress_params: WorkDoneProgressParams::default(),
                        partial_result_params: PartialResultParams::default(),
                    }),
                )
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "INVARIANT VIOLATED: goto_definition exceeded the bounded {:?} retrigger window after {retriggers} retriggers. This is a bug because a valid indexing generation must reach a target or terminal absence. Fix: inspect the stuck project phase and demand lifecycle.",
                        GOTO_DEFINITION_RETRIGGER_TIMEOUT
                    )
                });
                match result {
                    Ok(response) => break response,
                    Err(error)
                        if error.code == ErrorCode::ServerError(-32802)
                            && error.data.as_ref().is_some_and(|data| {
                                data.get("retriggerRequest")
                                    .and_then(serde_json::Value::as_bool)
                                    == Some(true)
                            })
                            && tokio::time::Instant::now() < deadline =>
                    {
                        retriggers += 1;
                        tokio::time::sleep(GOTO_DEFINITION_RETRIGGER_BACKOFF).await;
                    }
                    Err(error) => {
                        let indexing_snapshots = self
                            .server
                            .workspaces
                            .read()
                            .iter()
                            .map(|workspace| workspace.indexing_status.snapshot())
                            .collect::<Vec<_>>();
                        panic!(
                            "INVARIANT VIOLATED: goto_definition request failed after {retriggers} retriggers: {error:?}; indexing snapshots: {indexing_snapshots:#?}. This is a bug because FakeEditor accepts only the server's exact bounded retrigger contract during indexing. Fix: inspect the request error and indexing failure, or make the fixture reach a terminal stage."
                        )
                    }
                }
            }
        };

        match response {
            Some(GotoDefinitionResponse::Scalar(location)) => vec![location],
            Some(GotoDefinitionResponse::Array(locations)) => locations,
            Some(GotoDefinitionResponse::Link(_)) => panic!(
                "INVARIANT VIOLATED: goto_definition returned location links. This is a bug because Ruby Fast LSP currently returns locations. Fix: update the black-box harness when the server advertises location-link responses."
            ),
            None => Vec::new(),
        }
    }

    pub async fn hover(&self, filename: &str, line: u32, character: u32) -> Option<Hover> {
        self.assert_open(filename, "hover");
        let uri = filename_to_uri(filename);
        self.server
            .hover(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: Position { line, character },
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
            })
            .await
            .expect("INVARIANT VIOLATED: hover request failed. This is a bug because FakeEditor expects in-process LSP calls to return JSON-RPC success. Fix: inspect request handler error path.")
    }

    pub async fn completion_after_dot(
        &self,
        filename: &str,
        line: u32,
        character: u32,
    ) -> Vec<CompletionItem> {
        self.assert_open(filename, "completion_after_dot");
        let uri = filename_to_uri(filename);
        let response = self
            .server
            .completion(CompletionParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: Position { line, character },
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
                context: Some(CompletionContext {
                    trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
                    trigger_character: Some(".".to_string()),
                }),
            })
            .await
            .expect("INVARIANT VIOLATED: completion request failed. This is a bug because FakeEditor expects in-process LSP calls to return JSON-RPC success. Fix: inspect request handler error path.");
        match response {
            Some(CompletionResponse::Array(items)) => items,
            Some(CompletionResponse::List(list)) => list.items,
            None => Vec::new(),
        }
    }

    pub async fn references(&self, filename: &str, line: u32, character: u32) -> Vec<Location> {
        self.assert_open(filename, "references");
        let uri = filename_to_uri(filename);
        self.server
            .references(ReferenceParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: Position { line, character },
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
                context: ReferenceContext {
                    include_declaration: true,
                },
            })
            .await
            .expect("INVARIANT VIOLATED: references request failed. This is a bug because FakeEditor expects in-process LSP calls to return JSON-RPC success. Fix: inspect request handler error path.")
            .unwrap_or_default()
    }

    pub async fn extension_status(&self) -> Vec<ExtensionStatusReport> {
        self.server
            .handle_extension_status(ExtensionStatusParams::default())
            .await
            .expect("INVARIANT VIOLATED: extension status request failed. This is a bug because FakeEditor expects in-process LSP custom requests to return JSON-RPC success. Fix: inspect extension status handler error path.")
            .extensions
    }

    pub fn content(&self, filename: &str) -> &str {
        self.assert_open(filename, "content");
        &self.buffers[filename].0
    }

    fn assert_open(&self, filename: &str, operation: &str) {
        assert!(
            self.buffers.contains_key(filename),
            "INVARIANT VIOLATED: cannot {} unopened file `{}`. \
             This is a bug because FakeEditor operations require didOpen state. \
             Fix: call open() before querying.",
            operation,
            filename
        );
    }
}

pub fn filename_to_uri(filename: &str) -> Url {
    Url::parse(&format!("file:///{}", filename.trim_start_matches('/')))
        .expect("INVARIANT VIOLATED: FakeEditor built invalid file URI. This is a bug because test filenames must map to file:// URIs. Fix: sanitize filename_to_uri input.")
}
