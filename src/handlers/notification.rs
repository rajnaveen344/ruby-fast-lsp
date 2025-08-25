use std::sync::Arc;

use crate::capabilities;
use crate::config::RubyFastLspConfig;
use crate::handlers::helpers::{
    init_workspace, process_content_for_definitions, process_content_for_references,
};
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;
use crate::types::ruby_version::RubyVersion;
use log::{debug, info, warn};
use parking_lot::RwLock;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    DidChangeConfigurationParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, DidChangeWatchedFilesParams,
    InitializeParams, InitializedParams, *,
};

pub async fn handle_initialize(
    lang_server: &RubyLanguageServer,
    params: InitializeParams,
) -> LspResult<InitializeResult> {
    let workspace_folders = params.workspace_folders;

    // Process initialization options for configuration
    if let Some(init_options) = params.initialization_options {
        if let Ok(config) = serde_json::from_value::<RubyFastLspConfig>(init_options) {
            debug!("Received configuration: {:?}", config);
            *lang_server.config.lock() = config;
        } else {
            warn!("Failed to parse initialization options as configuration");
        }
    }

    if let Some(folder) = workspace_folders.and_then(|folders| folders.first().cloned()) {
        debug!(
            "Indexing workspace folder using workspace folder: {:?}",
            folder.uri.as_str()
        );
        let _ = init_workspace(lang_server, folder.uri.clone()).await;
    } else if let Some(root_uri) = params.root_uri {
        debug!(
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
        document_symbol_provider: Some(OneOf::Left(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        inlay_hint_provider: Some(OneOf::Right(
            capabilities::inlay_hints::get_inlay_hints_capability(),
        )),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            capabilities::semantic_tokens::get_semantic_tokens_options(),
        )),
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(true),
            trigger_characters: Some(vec![
                ":".to_string(), // Trigger on ":" to handle "::" for constant completion
                ".".to_string(), // Trigger on "." for method completion (future enhancement)
            ]),
            completion_item: Some(CompletionOptionsCompletionItem {
                label_details_support: Some(true),
            }),
            ..CompletionOptions::default()
        }),
        document_on_type_formatting_provider: Some(
            capabilities::formatting::get_document_on_type_formatting_options(),
        ),
        ..ServerCapabilities::default()
    };

    Ok(InitializeResult {
        capabilities,
        ..InitializeResult::default()
    })
}

pub async fn handle_initialized(server: &RubyLanguageServer, _params: InitializedParams) {
    info!("Language server initialized");

    let config = server.config.lock().clone();

    // Determine Ruby version based on configuration
    let ruby_version = if let Some(version) = config.get_ruby_version() {
        info!("Using configured Ruby version: {:?}", version);
        version
    } else {
        // Auto-detect Ruby version
        detect_system_ruby_version().unwrap_or_else(|| {
            info!("No Ruby version detected, using default Ruby 3.0");
            (3, 0)
        })
    };

    info!("Using Ruby version: {}.{}", ruby_version.0, ruby_version.1);
}

/// Simple system Ruby version detection without workspace context
fn detect_system_ruby_version() -> Option<(u8, u8)> {
    if let Ok(output) = std::process::Command::new("ruby")
        .args(&["--version"])
        .output()
    {
        if output.status.success() {
            let version_output = String::from_utf8_lossy(&output.stdout);
            // Parse output like "ruby 3.0.0p0 (2020-12-25 revision 95aff21468) [x86_64-darwin20]"
            if let Some(version_part) = version_output.split_whitespace().nth(1) {
                debug!("System ruby version output: {}", version_part);
                if let Some(version) = RubyVersion::from_full_version(version_part) {
                    return Some((version.major, version.minor));
                }
            }
        }
    }
    None
}

pub async fn handle_did_open(lang_server: &RubyLanguageServer, params: DidOpenTextDocumentParams) {
    let uri = params.text_document.uri.clone();
    let content = params.text_document.text.clone();

    let document = RubyDocument::new(uri.clone(), content.clone(), params.text_document.version);

    lang_server
        .docs
        .lock()
        .insert(uri.clone(), Arc::new(RwLock::new(document.clone())));
    debug!("Doc cache size: {}", lang_server.docs.lock().len());

    let _ = process_content_for_definitions(lang_server, uri.clone(), &content);
    let _ = process_content_for_references(lang_server, uri.clone(), &content, true);

    // Invalidate namespace tree cache with debouncing since new definitions may have been added
    lang_server.invalidate_namespace_tree_cache_debounced();
    debug!("Namespace tree cache invalidation scheduled due to new definitions");

    // Generate and publish diagnostics
    let diagnostics = capabilities::diagnostics::generate_diagnostics(&document);
    lang_server.publish_diagnostics(uri, diagnostics).await;
}

pub async fn handle_did_change(
    lang_server: &RubyLanguageServer,
    params: DidChangeTextDocumentParams,
) {
    let uri = params.text_document.uri.clone();
    let version = params.text_document.version;

    // Process all changes and get the final content
    let final_content = if let Some(last_change) = params.content_changes.last() {
        last_change.text.clone()
    } else {
        return; // No changes to process
    };

    // Update or create the document atomically
    let doc = {
        let mut docs = lang_server.docs.lock();
        if let Some(existing_doc) = docs.get(&uri) {
            // Update existing document
            let mut doc_guard = existing_doc.write();
            doc_guard.update(final_content.clone(), version);
            doc_guard.clone()
        } else {
            // Create new document if it doesn't exist
            let new_doc = RubyDocument::new(uri.clone(), final_content.clone(), version);
            docs.insert(uri.clone(), Arc::new(RwLock::new(new_doc.clone())));
            new_doc
        }
    };

    let _ = process_content_for_definitions(lang_server, uri.clone(), &final_content);
    let _ = process_content_for_references(lang_server, uri.clone(), &final_content, true);

    // Invalidate namespace tree cache with debouncing since the index has changed
    lang_server.invalidate_namespace_tree_cache_debounced();
    debug!("Namespace tree cache invalidation scheduled due to index change");

    // Generate and publish diagnostics
    let diagnostics = capabilities::diagnostics::generate_diagnostics(&doc);
    lang_server
        .publish_diagnostics(uri.clone(), diagnostics)
        .await;
}

pub async fn handle_did_close(
    lang_server: &RubyLanguageServer,
    params: DidCloseTextDocumentParams,
) {
    let uri = params.text_document.uri.clone();
    
    // Only remove the document from the in-memory cache, but keep definitions and references
    // in the index since the file still exists on disk and other files may reference it
    lang_server.docs.lock().remove(&uri);
    debug!("Doc cache size: {}", lang_server.docs.lock().len());

    // Clear diagnostics for the closed document
    lang_server.publish_diagnostics(uri, vec![]).await;
}

pub async fn handle_did_change_configuration(
    server: &RubyLanguageServer,
    params: DidChangeConfigurationParams,
) {
    info!("Configuration change received");

    // Extract the configuration from the settings
    if let Some(settings) = params.settings.as_object() {
        if let Some(ruby_fast_lsp_settings) = settings.get("rubyFastLsp") {
            if let Ok(config) =
                serde_json::from_value::<RubyFastLspConfig>(ruby_fast_lsp_settings.clone())
            {
                info!("Updated configuration: {:?}", config);

                // Update the server configuration
                *server.config.lock() = config.clone();

                // Handle configuration change for Ruby version
                let ruby_version = if let Some(version) = config.get_ruby_version() {
                    info!("Using configured Ruby version: {:?}", version);
                    version
                } else {
                    // Auto-detect Ruby version
                    detect_system_ruby_version().unwrap_or_else(|| {
                        info!("No Ruby version detected, using default Ruby 3.0");
                        (3, 0)
                    })
                };

                info!(
                    "Configuration updated with Ruby version: {:?}",
                    ruby_version
                );
            } else {
                warn!("Failed to parse configuration from settings");
            }
        }
    }
}

pub async fn handle_shutdown(_: &RubyLanguageServer) -> LspResult<()> {
    info!("Shutting down Ruby LSP server");
    Ok(())
}

pub async fn handle_did_save(lang_server: &RubyLanguageServer, params: DidSaveTextDocumentParams) {
    let uri = params.text_document.uri;
    debug!("Document saved: {}", uri.path());
    
    // Check if it's a Ruby file
    if uri.path().ends_with(".rb") {
        // Trigger debounced cache invalidation for namespace tree
        lang_server.invalidate_namespace_tree_cache_debounced();
        debug!("Scheduled namespace tree cache invalidation for saved file: {}", uri.path());
    }
}

pub async fn handle_did_change_watched_files(lang_server: &RubyLanguageServer, params: DidChangeWatchedFilesParams) {
    debug!("Watched files changed: {} files", params.changes.len());
    
    let mut has_ruby_changes = false;
    for change in &params.changes {
        debug!("File change: {:?} - {:?}", change.uri.path(), change.typ);
        if change.uri.path().ends_with(".rb") {
            has_ruby_changes = true;
        }
    }
    
    // If any Ruby files changed, trigger debounced cache invalidation
    if has_ruby_changes {
        lang_server.invalidate_namespace_tree_cache_debounced();
        debug!("Scheduled namespace tree cache invalidation for watched file changes");
    }
}
