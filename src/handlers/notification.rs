use crate::capabilities;
use crate::handlers::helpers::{init_workspace, process_file_for_indexing};
use crate::server::RubyLanguageServer;
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
    // Did open handler started

    let uri = params.text_document.uri.clone();
    let content = params.text_document.text.clone();

    // Process the file for indexing
    let index_ref = lang_server.index();
    let result = process_file_for_indexing(index_ref, uri.clone(), &content);

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
    let index_ref = lang_server.index();

    for change in params.content_changes {
        let content = change.text.clone();
        let result = process_file_for_indexing(index_ref.clone(), uri.clone(), &content);

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
    let content = std::fs::read_to_string(uri.to_file_path().unwrap()).unwrap();
    let index_ref = lang_server.index();
    let result = process_file_for_indexing(index_ref, uri, &content);

    if let Err(e) = result {
        error!("Error re-indexing document: {}", e);
    }
}

pub async fn handle_shutdown(_: &RubyLanguageServer) -> LspResult<()> {
    info!("Shutting down Ruby LSP server");
    Ok(())
}
