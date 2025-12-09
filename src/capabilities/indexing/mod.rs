pub mod processor;

use crate::capabilities;
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;
use log::{debug, info};
use parking_lot::RwLock;
use std::sync::Arc;
use tower_lsp::lsp_types::*;

use self::processor::{get_unresolved_diagnostics, process_file, ReferenceOptions};

/// Initialize workspace and run complete indexing
pub async fn init_workspace(server: &RubyLanguageServer, folder_uri: Url) -> anyhow::Result<()> {
    processor::init_workspace(server, folder_uri).await
}

pub async fn handle_did_open(server: &RubyLanguageServer, params: DidOpenTextDocumentParams) {
    let uri = params.text_document.uri.clone();
    let content = params.text_document.text.clone();

    let document = RubyDocument::new(uri.clone(), content.clone(), params.text_document.version);

    server
        .docs
        .lock()
        .insert(uri.clone(), Arc::new(RwLock::new(document)));
    debug!("Doc cache size: {}", server.docs.lock().len());

    // Track file for type narrowing analysis
    server.type_narrowing.on_file_open(&uri, &content);

    // Process file with single parse (definitions + references + diagnostics)
    // This avoids re-parsing the file for diagnostics
    let (affected_uris, mut diagnostics) = match process_file(
        server,
        uri.clone(),
        &content,
        false, // resolve mixins on open
        ReferenceOptions::default().with_unresolved_tracking(true),
    ) {
        Ok(result) => (result.affected_uris, result.syntax_diagnostics),
        Err(_) => (std::collections::HashSet::new(), Vec::new()),
    };

    // Invalidate namespace tree cache with debouncing
    server.invalidate_namespace_tree_cache_debounced();
    debug!("Namespace tree cache invalidation scheduled due to new definitions");

    // Add unresolved entry diagnostics and YARD diagnostics (no re-parsing needed)
    diagnostics.extend(get_unresolved_diagnostics(server, &uri));
    {
        let index = server.index.lock();
        diagnostics.extend(capabilities::diagnostics::generate_yard_diagnostics(
            &index, &uri,
        ));
    }
    server.publish_diagnostics(uri.clone(), diagnostics).await;

    // Publish diagnostics for files affected by removed definitions (cross-file propagation)
    for affected_uri in affected_uris {
        if affected_uri != uri {
            let affected_diagnostics = get_unresolved_diagnostics(server, &affected_uri);
            server
                .publish_diagnostics(affected_uri, affected_diagnostics)
                .await;
        }
    }
}

pub async fn handle_did_change(server: &RubyLanguageServer, params: DidChangeTextDocumentParams) {
    let uri = params.text_document.uri.clone();
    let version = params.text_document.version;

    // Get the final content from the last change
    let final_content = match params.content_changes.last() {
        Some(change) => change.text.clone(),
        None => return,
    };

    // Update or create the document atomically
    {
        let mut docs = server.docs.lock();
        if let Some(existing_doc) = docs.get(&uri) {
            let mut doc_guard = existing_doc.write();
            doc_guard.update(final_content.clone(), version);
        } else {
            let new_doc = RubyDocument::new(uri.clone(), final_content.clone(), version);
            docs.insert(uri.clone(), Arc::new(RwLock::new(new_doc)));
        }
    }

    // Update type narrowing engine with new content
    server.type_narrowing.on_file_change(&uri, &final_content);

    // INSTANT PROCESSING: Do full indexing on every change (no debouncing)
    // This provides immediate feedback for completions/definitions
    // Skip mixin resolution and unresolved tracking for speed - defer to save
    let (affected_uris, mut diagnostics) = match process_file(
        server,
        uri.clone(),
        &final_content,
        true, // skip mixin resolution on change
        ReferenceOptions::default().with_unresolved_tracking(false),
    ) {
        Ok(result) => (result.affected_uris, result.syntax_diagnostics),
        Err(_) => (std::collections::HashSet::new(), Vec::new()),
    };

    // Add unresolved diagnostics (fast - just index lookup)
    diagnostics.extend(get_unresolved_diagnostics(server, &uri));

    server.publish_diagnostics(uri.clone(), diagnostics).await;

    // Invalidate namespace tree cache with debouncing
    server.invalidate_namespace_tree_cache_debounced();
    debug!("Namespace tree cache invalidation scheduled due to index change");

    // Publish diagnostics for affected files (cross-file propagation)
    for affected_uri in affected_uris {
        if affected_uri != uri {
            let affected_diagnostics = get_unresolved_diagnostics(server, &affected_uri);
            server
                .publish_diagnostics(affected_uri, affected_diagnostics)
                .await;
        }
    }
}

pub async fn handle_did_save(server: &RubyLanguageServer, params: DidSaveTextDocumentParams) {
    let uri = params.text_document.uri;
    info!("Document saved: {}", uri.path());

    if !uri.path().ends_with(".rb") {
        return;
    }

    // Get the current document content
    let content = {
        let docs = server.docs.lock();
        match docs.get(&uri) {
            Some(doc_arc) => doc_arc.read().content.clone(),
            None => return,
        }
    };

    // On save: do full indexing with unresolved tracking (for cross-file diagnostics)
    // This is the only place we track unresolved references for accurate diagnostics
    let (affected_uris, mut diagnostics) = match process_file(
        server,
        uri.clone(),
        &content,
        false, // resolve mixins on save
        ReferenceOptions::default().with_unresolved_tracking(true),
    ) {
        Ok(result) => (result.affected_uris, result.syntax_diagnostics),
        Err(_) => (std::collections::HashSet::new(), Vec::new()),
    };

    // Invalidate namespace tree cache
    server.invalidate_namespace_tree_cache_debounced();

    // Add unresolved diagnostics and YARD diagnostics (no re-parsing needed)
    diagnostics.extend(get_unresolved_diagnostics(server, &uri));
    {
        let index = server.index.lock();
        diagnostics.extend(capabilities::diagnostics::generate_yard_diagnostics(
            &index, &uri,
        ));
    }
    server.publish_diagnostics(uri.clone(), diagnostics).await;

    // Publish diagnostics for files affected by removed definitions
    for affected_uri in affected_uris {
        if affected_uri != uri {
            let affected_diagnostics = get_unresolved_diagnostics(server, &affected_uri);
            server
                .publish_diagnostics(affected_uri, affected_diagnostics)
                .await;
        }
    }

    // Request the client to refresh inlay hints after save
    server.refresh_inlay_hints().await;
}

pub async fn handle_did_close(server: &RubyLanguageServer, params: DidCloseTextDocumentParams) {
    let uri = params.text_document.uri.clone();

    // Remove the document from in-memory cache but keep definitions/references in the index
    server.docs.lock().remove(&uri);
    debug!("Doc cache size: {}", server.docs.lock().len());

    // Remove type narrowing CFG cache for this file
    server.type_narrowing.on_file_close(&uri);

    // Keep unresolved entry diagnostics visible (project-wide diagnostics)
    let diagnostics = get_unresolved_diagnostics(server, &uri);
    server.publish_diagnostics(uri, diagnostics).await;
}

pub async fn handle_watched_files_changed(
    server: &RubyLanguageServer,
    params: DidChangeWatchedFilesParams,
) {
    debug!("Watched files changed: {} files", params.changes.len());

    let has_ruby_changes = params
        .changes
        .iter()
        .any(|change| change.uri.path().ends_with(".rb"));

    if has_ruby_changes {
        server.invalidate_namespace_tree_cache_debounced();
        debug!("Scheduled namespace tree cache invalidation for watched file changes");
    }
}
