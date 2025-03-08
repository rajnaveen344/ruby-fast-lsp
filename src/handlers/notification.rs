use crate::indexer::events;
use crate::server::RubyLanguageServer;
use lsp_types::*;

pub async fn handle_did_open(state: &RubyLanguageServer, params: DidOpenTextDocumentParams) {
    let uri = params.text_document.uri.clone();
    let text = params.text_document.text.clone();

    // Update the document cache
    state
        .update_document_content(uri.clone(), text.clone())
        .await;

    // Index the document
    let mut indexer = state.indexer.lock().await;
    if let Err(e) = events::handle_did_open(&mut indexer, uri.clone(), &text) {
        state
            .client
            .log_message(
                MessageType::ERROR,
                format!("Error indexing document: {}", e),
            )
            .await;
    }
}

pub async fn handle_did_change(state: &RubyLanguageServer, params: DidChangeTextDocumentParams) {
    let uri = params.text_document.uri.clone();

    // For full sync, we get the full content in the first change
    if let Some(change) = params.content_changes.first() {
        let text = change.text.clone();

        // Update the document cache
        state
            .update_document_content(uri.clone(), text.clone())
            .await;

        // Re-index the document with new content
        let mut indexer = state.indexer.lock().await;
        if let Err(e) = events::handle_did_change(&mut indexer, uri.clone(), &text) {
            state
                .client
                .log_message(
                    MessageType::ERROR,
                    format!("Error re-indexing document: {}", e),
                )
                .await;
        }
    }
}

pub async fn handle_did_close(state: &RubyLanguageServer, params: DidCloseTextDocumentParams) {
    let uri = params.text_document.uri.clone();

    // Remove from cache
    state.remove_document(&uri).await;

    // Remove from indexer
    let mut indexer = state.indexer.lock().await;
    events::handle_did_close(&mut indexer, &uri);
}
