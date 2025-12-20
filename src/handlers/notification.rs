//! LSP Notification Handlers
//!
//! This module contains handlers for LSP notifications (events that don't require a response).
//! All helper functions and business logic should be in `helpers.rs`.

use crate::capabilities::{self, indexing};
use crate::config::RubyFastLspConfig;
use crate::server::RubyLanguageServer;
use crate::utils::detect_system_ruby_version;
use log::{debug, info, warn};
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
            lang_server.set_parent_process_id(Some(process_id));
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
        hover_provider: Some(HoverProviderCapability::Simple(true)),
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

            let result = indexing::init_workspace(&server_clone, workspace_uri.clone()).await;

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

pub async fn handle_did_open(server: &RubyLanguageServer, params: DidOpenTextDocumentParams) {
    indexing::handle_did_open(server, params).await;
}

pub async fn handle_did_change(server: &RubyLanguageServer, params: DidChangeTextDocumentParams) {
    indexing::handle_did_change(server, params).await;
}

pub async fn handle_did_close(server: &RubyLanguageServer, params: DidCloseTextDocumentParams) {
    indexing::handle_did_close(server, params).await;
}

pub async fn handle_did_save(server: &RubyLanguageServer, params: DidSaveTextDocumentParams) {
    indexing::handle_did_save(server, params).await;
}

pub async fn handle_did_change_watched_files(
    server: &RubyLanguageServer,
    params: DidChangeWatchedFilesParams,
) {
    indexing::handle_watched_files_changed(server, params).await;
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
