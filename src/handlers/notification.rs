use crate::indexer::events;
use crate::server::RubyLanguageServer;
use log::{debug, error, info};
use lsp_types::*;
use std::time::Instant;

pub async fn handle_initialized(_: &RubyLanguageServer, _: InitializedParams) {
    info!("Server initialized");
}

pub async fn handle_did_open(lang_server: &RubyLanguageServer, params: DidOpenTextDocumentParams) {
    let start_time = Instant::now();
    // Did open handler started

    let uri = params.text_document.uri.clone();
    let content = params.text_document.text.clone();
    let mut indexer = lang_server.indexer.lock().await;
    let res = events::file_opened(&mut indexer, uri, &content);

    if let Err(e) = res {
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
        let mut indexer = lang_server.indexer.lock().await;
        let res = events::file_changed(&mut indexer, uri.clone(), &content);

        if let Err(e) = res {
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
    let mut indexer = lang_server.indexer.lock().await;
    let content = std::fs::read_to_string(uri.to_file_path().unwrap()).unwrap();
    let res = events::file_changed(&mut indexer, uri, &content);

    if let Err(e) = res {
        error!("Error re-indexing document: {}", e);
    }
}
