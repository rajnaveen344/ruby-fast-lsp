use crate::capabilities;
use crate::handlers::helpers::{init_workspace, process_file_for_indexing};
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;
use log::{debug, error, info, warn};
use lsp_types::*;
use std::time::Instant;
use tower_lsp::jsonrpc::Result as LspResult;

pub async fn handle_initialize(
    lang_server: &RubyLanguageServer,
    params: InitializeParams,
) -> LspResult<InitializeResult> {
    info!("Initializing Ruby LSP server");

    let workspace_folders = params.workspace_folders;

    if let Some(folder) = workspace_folders.and_then(|folders| folders.first().cloned()) {
        info!(
            "Indexing workspace folder using workspace folder: {:?}",
            folder.uri.as_str()
        );
        let _ = init_workspace(lang_server, folder.uri.clone()).await;
    } else if let Some(root_uri) = params.root_uri {
        info!(
            "Indexing workspace folder using root URI: {:?}",
            root_uri.as_str()
        );
        let _ = init_workspace(lang_server, root_uri.clone()).await;
    } else {
        warn!("No workspace folder or root URI provided. A workspace folder is required to function properly");
    }

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        inlay_hint_provider: Some(OneOf::Right(
            capabilities::inlay_hints::get_inlay_hints_capability(),
        )),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            capabilities::semantic_tokens::get_semantic_tokens_options(),
        )),
        ..ServerCapabilities::default()
    };

    Ok(InitializeResult {
        capabilities,
        ..InitializeResult::default()
    })
}

pub async fn handle_initialized(_: &RubyLanguageServer, _: InitializedParams) {
    info!("Server initialized");
}

pub async fn handle_did_open(lang_server: &RubyLanguageServer, params: DidOpenTextDocumentParams) {
    let start_time = Instant::now();
    let uri = params.text_document.uri.clone();
    let content = params.text_document.text.clone();

    lang_server.docs.lock().unwrap().insert(
        uri.clone(),
        RubyDocument::new(uri.clone(), content.clone(), params.text_document.version),
    );

    let result = process_file_for_indexing(lang_server, uri.clone());

    if let Err(e) = result {
        error!("Error indexing document: {}", e);
    }

    debug!("[PERF] File indexed in {:?}", start_time.elapsed());
}

pub async fn handle_did_change(
    lang_server: &RubyLanguageServer,
    params: DidChangeTextDocumentParams,
) {
    debug!("Did change: {:?}", params.text_document.uri.as_str());
    let uri = params.text_document.uri.clone();

    for change in params.content_changes {
        let content = change.text.clone();
        let doc = RubyDocument::new(uri.clone(), content.clone(), params.text_document.version);

        lang_server.docs.lock().unwrap().insert(uri.clone(), doc);

        let result = process_file_for_indexing(lang_server, uri.clone());

        if let Err(e) = result {
            error!("Error re-indexing document: {}", e);
        }
    }
}

pub async fn handle_did_close(
    lang_server: &RubyLanguageServer,
    params: DidCloseTextDocumentParams,
) {
    debug!("Did close: {:?}", params.text_document.uri.as_str());
    let uri = params.text_document.uri.clone();
    let result = process_file_for_indexing(lang_server, uri);

    if let Err(e) = result {
        error!("Error re-indexing document: {}", e);
    }
}

pub async fn handle_shutdown(_: &RubyLanguageServer) -> LspResult<()> {
    info!("Shutting down Ruby LSP server");
    Ok(())
}
