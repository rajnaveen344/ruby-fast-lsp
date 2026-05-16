use std::collections::HashMap;

use ruby_fast_lsp::extensions::{ExtensionStatusParams, ExtensionStatusReport};
use ruby_fast_lsp::server::RubyLanguageServer;
use tower_lsp::lsp_types::{
    CodeLens, CodeLensParams, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, InitializeParams,
    PartialResultParams, TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    Url, VersionedTextDocumentIdentifier, WorkDoneProgressParams,
};
use tower_lsp::LanguageServer;

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
