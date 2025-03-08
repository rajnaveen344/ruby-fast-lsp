use crate::indexer::events;
use crate::server::RubyLanguageServer;
use lsp_types::*;

pub async fn handle_initialized(lang_server: &RubyLanguageServer, _: InitializedParams) {
    if let Some(client) = lang_server.client.clone() {
        client
            .log_message(MessageType::INFO, "Server initialized")
            .await;
    }
}

pub async fn handle_did_open(lang_server: &RubyLanguageServer, params: DidOpenTextDocumentParams) {
    let uri = params.text_document.uri.clone();
    let text = params.text_document.text.clone();

    // Update the document cache
    lang_server
        .update_document_content(uri.clone(), text.clone())
        .await;

    // Index the document
    let mut indexer = lang_server.indexer.lock().await;
    if let Err(e) = events::handle_did_open(&mut indexer, uri.clone(), &text) {
        if let Some(client) = lang_server.client.clone() {
            client
                .log_message(
                    MessageType::ERROR,
                    format!("Error indexing document: {}", e),
                )
                .await;
        }
    }
}

pub async fn handle_did_change(
    lang_server: &RubyLanguageServer,
    params: DidChangeTextDocumentParams,
) {
    let uri = params.text_document.uri.clone();

    // For full sync, we get the full content in the first change
    if let Some(change) = params.content_changes.first() {
        let text = change.text.clone();

        // Update the document cache
        lang_server
            .update_document_content(uri.clone(), text.clone())
            .await;

        // Re-index the document with new content
        let mut indexer = lang_server.indexer.lock().await;
        if let Err(e) = events::handle_did_change(&mut indexer, uri.clone(), &text) {
            if let Some(client) = lang_server.client.clone() {
                client
                    .log_message(
                        MessageType::ERROR,
                        format!("Error re-indexing document: {}", e),
                    )
                    .await;
            }
        }
    }
}

pub async fn handle_did_close(
    lang_server: &RubyLanguageServer,
    params: DidCloseTextDocumentParams,
) {
    let uri = params.text_document.uri.clone();

    // Remove from cache
    lang_server.remove_document(&uri).await;

    // Remove from indexer
    let mut indexer = lang_server.indexer.lock().await;
    events::handle_did_close(&mut indexer, &uri);
}
