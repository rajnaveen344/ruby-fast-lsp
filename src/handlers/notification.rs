//! LSP Notification Handlers
//!
//! This module contains handlers for LSP notifications (events that don't require a response).
//! All helper functions and business logic should be in `helpers.rs`.

use std::sync::Arc;

use crate::capabilities;
use crate::config::RubyFastLspConfig;
use crate::handlers::helpers::{
    detect_system_ruby_version, get_unresolved_diagnostics, init_workspace, process_file,
    DefinitionOptions, ReferenceOptions,
};
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;
use log::{debug, info, warn};
use parking_lot::RwLock;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;

pub async fn handle_initialize(
    lang_server: &RubyLanguageServer,
    params: InitializeParams,
) -> LspResult<InitializeResult> {
    let workspace_folders = params.workspace_folders;
    let root_uri = params.root_uri;

    // Extract and monitor parent process ID to detect when VS Code dies
    // This ensures the LSP server exits when the extension is uninstalled/reloaded
    if let Some(process_id) = params.process_id {
        if process_id > 0 {
            info!(
                "Parent process ID received: {}. Starting process monitor.",
                process_id
            );
            lang_server.set_parent_process_id(Some(process_id as u32));
        } else {
            info!(
                "Invalid parent process ID received ({}), skipping process monitoring",
                process_id
            );
        }
    } else {
        info!("No parent process ID received, skipping process monitoring");
    }

    // Process initialization options for configuration
    if let Some(init_options) = params.initialization_options {
        if let Ok(config) = serde_json::from_value::<RubyFastLspConfig>(init_options) {
            debug!("Received configuration: {:?}", config);
            *lang_server.config.lock() = config;
        } else {
            warn!("Failed to parse initialization options as configuration");
        }
    }

    // Store the workspace folder/root URI for later use in initialized
    if let Some(folder) = workspace_folders.and_then(|folders| folders.first().cloned()) {
        info!(
            "Will index workspace folder after initialization: {:?}",
            folder.uri.as_str()
        );
        lang_server.set_workspace_uri(Some(folder.uri.clone()));
    } else if let Some(root) = root_uri {
        info!(
            "Will index workspace folder after initialization: {:?}",
            root.as_str()
        );
        lang_server.set_workspace_uri(Some(root.clone()));
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
        code_lens_provider: Some(CodeLensOptions {
            resolve_provider: Some(false),
        }),
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
        detect_system_ruby_version().unwrap_or_else(|| {
            info!("No Ruby version detected, using default Ruby 3.0");
            (3, 0)
        })
    };

    info!("Using Ruby version: {}.{}", ruby_version.0, ruby_version.1);

    // Run workspace indexing in the background
    if let Some(workspace_uri) = server.get_workspace_uri() {
        let server_clone = server.clone();
        tokio::spawn(async move {
            info!("Starting background workspace indexing");

            // Send progress begin notification
            if let Some(client) = &server_clone.client {
                let _ = client
                    .send_notification::<notification::Progress>(ProgressParams {
                        token: NumberOrString::String("indexing".to_string()),
                        value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(
                            WorkDoneProgressBegin {
                                title: "Ruby Fast LSP".to_string(),
                                message: Some("Indexing workspace...".to_string()),
                                percentage: Some(0),
                                cancellable: Some(false),
                            },
                        )),
                    })
                    .await;
            }

            let result = init_workspace(&server_clone, workspace_uri.clone()).await;

            // Send completion notification
            if let Some(client) = &server_clone.client {
                match result {
                    Ok(_) => {
                        info!("Background workspace indexing completed successfully");
                        let _ = client
                            .send_notification::<notification::Progress>(ProgressParams {
                                token: NumberOrString::String("indexing".to_string()),
                                value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(
                                    WorkDoneProgressEnd {
                                        message: Some("Indexing complete".to_string()),
                                    },
                                )),
                            })
                            .await;

                        let _ = client
                            .show_message(
                                MessageType::INFO,
                                "Ruby Fast LSP: Workspace indexing complete",
                            )
                            .await;
                    }
                    Err(e) => {
                        warn!("Background workspace indexing failed: {}", e);
                        let _ = client
                            .send_notification::<notification::Progress>(ProgressParams {
                                token: NumberOrString::String("indexing".to_string()),
                                value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(
                                    WorkDoneProgressEnd {
                                        message: Some(format!("Indexing failed: {}", e)),
                                    },
                                )),
                            })
                            .await;
                    }
                }
            }
        });

        info!("Background workspace indexing task spawned, LSP is now ready for requests");
    }
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

    // Process file with single parse (definitions + references)
    let affected_uris = match process_file(
        lang_server,
        uri.clone(),
        &content,
        DefinitionOptions::default(),
        ReferenceOptions::default().with_unresolved_tracking(true),
    ) {
        Ok(result) => result.affected_uris,
        Err(_) => std::collections::HashSet::new(),
    };

    // Invalidate namespace tree cache with debouncing
    lang_server.invalidate_namespace_tree_cache_debounced();
    debug!("Namespace tree cache invalidation scheduled due to new definitions");

    // Generate and publish diagnostics (syntax errors + unresolved entries + YARD issues)
    let mut diagnostics = capabilities::diagnostics::generate_diagnostics(&document);
    diagnostics.extend(get_unresolved_diagnostics(lang_server, &uri));
    // Add YARD documentation diagnostics (e.g., @param for non-existent parameters)
    {
        let index = lang_server.index.lock();
        diagnostics.extend(capabilities::diagnostics::generate_yard_diagnostics(
            &index, &uri,
        ));
    }
    lang_server
        .publish_diagnostics(uri.clone(), diagnostics)
        .await;

    // Publish diagnostics for files affected by removed definitions (cross-file propagation)
    for affected_uri in affected_uris {
        if affected_uri != uri {
            let affected_diagnostics = get_unresolved_diagnostics(lang_server, &affected_uri);
            lang_server
                .publish_diagnostics(affected_uri, affected_diagnostics)
                .await;
        }
    }
}

pub async fn handle_did_change(
    lang_server: &RubyLanguageServer,
    params: DidChangeTextDocumentParams,
) {
    let uri = params.text_document.uri.clone();
    let version = params.text_document.version;

    // Get the final content from the last change
    let final_content = match params.content_changes.last() {
        Some(change) => change.text.clone(),
        None => return,
    };

    // Update or create the document atomically (lightweight - always do this)
    let doc = {
        let mut docs = lang_server.docs.lock();
        if let Some(existing_doc) = docs.get(&uri) {
            let mut doc_guard = existing_doc.write();
            doc_guard.update(final_content.clone(), version);
            doc_guard.clone()
        } else {
            let new_doc = RubyDocument::new(uri.clone(), final_content.clone(), version);
            docs.insert(uri.clone(), Arc::new(RwLock::new(new_doc.clone())));
            new_doc
        }
    };

    // Lightweight: Only generate syntax diagnostics (fast - no index lookup)
    let diagnostics = capabilities::diagnostics::generate_diagnostics(&doc);
    lang_server
        .publish_diagnostics(uri.clone(), diagnostics)
        .await;

    // Schedule debounced reindex for type inference (500ms delay)
    lang_server.schedule_reindex_debounced(uri.clone(), final_content);

    // Invalidate namespace tree cache with debouncing
    lang_server.invalidate_namespace_tree_cache_debounced();
    debug!("Namespace tree cache invalidation scheduled due to index change");
}

pub async fn handle_did_close(
    lang_server: &RubyLanguageServer,
    params: DidCloseTextDocumentParams,
) {
    let uri = params.text_document.uri.clone();

    // Remove the document from in-memory cache but keep definitions/references in the index
    lang_server.docs.lock().remove(&uri);
    debug!("Doc cache size: {}", lang_server.docs.lock().len());

    // Keep unresolved entry diagnostics visible (project-wide diagnostics like rust-analyzer)
    let diagnostics = get_unresolved_diagnostics(lang_server, &uri);
    lang_server.publish_diagnostics(uri, diagnostics).await;
}

pub async fn handle_did_save(lang_server: &RubyLanguageServer, params: DidSaveTextDocumentParams) {
    let start_time = std::time::Instant::now();
    let uri = params.text_document.uri;
    info!("Document saved: {}", uri.path());

    if !uri.path().ends_with(".rb") {
        return;
    }

    // Get the current document content
    let (doc, content) = {
        let docs = lang_server.docs.lock();
        match docs.get(&uri) {
            Some(doc_arc) => {
                let doc = doc_arc.read().clone();
                let content = doc.content.clone();
                (doc, content)
            }
            None => return,
        }
    };

    // Do full indexing on save (heavy work deferred from did_change)
    let affected_uris = match process_file(
        lang_server,
        uri.clone(),
        &content,
        DefinitionOptions::default(),
        ReferenceOptions::default().with_unresolved_tracking(true),
    ) {
        Ok(result) => result.affected_uris,
        Err(_) => std::collections::HashSet::new(),
    };

    // Invalidate namespace tree cache
    lang_server.invalidate_namespace_tree_cache_debounced();

    // Generate and publish full diagnostics (syntax + unresolved + YARD)
    let mut diagnostics = capabilities::diagnostics::generate_diagnostics(&doc);
    diagnostics.extend(get_unresolved_diagnostics(lang_server, &uri));
    // Add YARD documentation diagnostics
    {
        let index = lang_server.index.lock();
        diagnostics.extend(capabilities::diagnostics::generate_yard_diagnostics(
            &index, &uri,
        ));
    }
    lang_server
        .publish_diagnostics(uri.clone(), diagnostics)
        .await;

    // Publish diagnostics for files affected by removed definitions
    for affected_uri in affected_uris {
        if affected_uri != uri {
            let affected_diagnostics = get_unresolved_diagnostics(lang_server, &affected_uri);
            lang_server
                .publish_diagnostics(affected_uri, affected_diagnostics)
                .await;
        }
    }

    // Request the client to refresh inlay hints after save
    lang_server.refresh_inlay_hints().await;

    info!(
        "[PERF] Document save handler completed in {:?}",
        start_time.elapsed()
    );
}

pub async fn handle_did_change_watched_files(
    lang_server: &RubyLanguageServer,
    params: DidChangeWatchedFilesParams,
) {
    debug!("Watched files changed: {} files", params.changes.len());

    let has_ruby_changes = params
        .changes
        .iter()
        .any(|change| change.uri.path().ends_with(".rb"));

    if has_ruby_changes {
        lang_server.invalidate_namespace_tree_cache_debounced();
        debug!("Scheduled namespace tree cache invalidation for watched file changes");
    }
}

pub async fn handle_did_change_configuration(
    server: &RubyLanguageServer,
    params: DidChangeConfigurationParams,
) {
    info!("Configuration change received");

    if let Some(settings) = params.settings.as_object() {
        if let Some(ruby_fast_lsp_settings) = settings.get("rubyFastLsp") {
            if let Ok(config) =
                serde_json::from_value::<RubyFastLspConfig>(ruby_fast_lsp_settings.clone())
            {
                info!("Updated configuration: {:?}", config);
                *server.config.lock() = config.clone();

                let ruby_version = if let Some(version) = config.get_ruby_version() {
                    info!("Using configured Ruby version: {:?}", version);
                    version
                } else {
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
