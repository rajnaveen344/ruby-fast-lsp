//! LSP Notification Handlers
//!
//! This module contains handlers for LSP notifications (events that don't require a response).
//! All helper functions and business logic should be in `helpers.rs`.

use crate::capabilities::{self, indexing};
use crate::config::RubyFastLspConfig;
use crate::server::RubyLanguageServer;
use crate::utils::detect_system_ruby_version;
use log::{debug, info, warn};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
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
            lang_server
                .extension_registry
                .configure_from_config(&config);
            *lang_server.config.lock() = config;
        } else {
            warn!("Failed to parse initialization options as configuration");
        }
    } else {
        lang_server
            .extension_registry
            .configure_from_config(&RubyFastLspConfig::default());
    }

    // Register every workspace folder. Each folder gets its own RubyIndex
    // and is indexed independently in handle_initialized. Multi-root VS Code
    // workspaces, Solargraph-style — folders do not bleed into one another.
    let folders: Vec<WorkspaceFolder> = workspace_folders.unwrap_or_default();
    if !folders.is_empty() {
        for folder in &folders {
            info!(
                "Registering workspace folder for indexing: {}",
                folder.uri.as_str()
            );
            lang_server.add_workspace(folder.uri.clone());
        }
    } else if let Some(root) = root_uri {
        info!("Registering workspace root for indexing: {}", root.as_str());
        lang_server.add_workspace(root.clone());
    } else {
        warn!("No workspace folder or root URI provided. Files opened ad-hoc will use the orphan index.");
    }

    // Build static capabilities
    // Note: Type hierarchy is dynamically registered in handle_initialized
    // because lsp-types 0.94.1 doesn't have typeHierarchyProvider field
    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        definition_provider: Some(OneOf::Left(true)),
        implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
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
        rename_provider: Some(OneOf::Left(true)),
        // Advertise multi-root workspace support so clients send
        // `workspace/didChangeWorkspaceFolders` for runtime add/remove.
        workspace: Some(WorkspaceServerCapabilities {
            workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                supported: Some(true),
                change_notifications: Some(OneOf::Left(true)),
            }),
            file_operations: None,
        }),
        ..ServerCapabilities::default()
    };

    Ok(InitializeResult {
        capabilities,
        server_info: Some(ServerInfo {
            name: "Ruby Fast LSP".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    })
}

pub async fn handle_initialized(server: &RubyLanguageServer, _params: InitializedParams) {
    info!("Language server initialized");

    // Dynamically register type hierarchy capability (LSP 3.17.0)
    // lsp-types 0.94.1 doesn't have typeHierarchyProvider in ServerCapabilities,
    // so we use dynamic registration to enable the "Show Type Hierarchy" menu option.
    if let Some(client) = &server.client {
        let registration = Registration {
            id: "type-hierarchy".to_string(),
            method: "textDocument/prepareTypeHierarchy".to_string(),
            register_options: Some(serde_json::json!({
                "documentSelector": [
                    { "language": "ruby" }
                ]
            })),
        };

        let call_hierarchy_registration = Registration {
            id: "call-hierarchy".to_string(),
            method: "textDocument/prepareCallHierarchy".to_string(),
            register_options: Some(serde_json::json!({
                "documentSelector": [
                    { "language": "ruby" }
                ]
            })),
        };

        match client
            .register_capability(vec![registration, call_hierarchy_registration])
            .await
        {
            Ok(_) => info!("Successfully registered type/call hierarchy capabilities"),
            Err(e) => warn!("Failed to register hierarchy capabilities: {:?}", e),
        }
    }

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

    // Spawn one coordinator per registered workspace. Each coordinator owns
    // a per-workspace `Index<Unlocked>` (created in `add_workspace`) and runs
    // independently. They share the server only for client notifications,
    // config, and document state.
    let workspaces = server.list_workspaces();
    if workspaces.is_empty() {
        info!("No workspaces registered; skipping background indexing");
        return;
    }

    if let Some(client) = &server.client {
        let _ = client
            .send_notification::<notification::Progress>(ProgressParams {
                token: NumberOrString::String("indexing".to_string()),
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(
                    WorkDoneProgressBegin {
                        title: "Ruby Fast LSP".to_string(),
                        message: Some(format!("Indexing {} workspace(s)...", workspaces.len())),
                        percentage: Some(0),
                        cancellable: Some(false),
                    },
                )),
            })
            .await;
    }

    let total = workspaces.len();
    let remaining = Arc::new(AtomicUsize::new(total));

    for ws in workspaces {
        let server_clone = server.clone();
        let remaining_clone = remaining.clone();
        tokio::spawn(async move {
            let workspace_uri = ws.root_uri.clone();
            info!(
                "Starting background indexing for workspace: {}",
                workspace_uri.as_str()
            );

            let result = indexing::init_workspace(&server_clone, workspace_uri.clone()).await;

            match result {
                Ok(_) => {
                    info!(
                        "Background indexing completed for workspace: {}",
                        workspace_uri.as_str()
                    );
                    ws.indexing_complete
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                Err(e) => {
                    warn!(
                        "Background indexing failed for workspace {}: {}",
                        workspace_uri.as_str(),
                        e
                    );
                }
            }

            // Last workspace to finish closes out the progress notification.
            let prev = remaining_clone.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            if prev == 1 {
                if let Some(client) = &server_clone.client {
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
            }
        });
    }

    info!(
        "Background indexing tasks spawned for {} workspace(s); LSP is now ready for requests",
        total
    );
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

/// Add or remove workspace folders at runtime in response to
/// `workspace/didChangeWorkspaceFolders`. Each added folder gets its own
/// `RubyIndex` and a freshly spawned indexing coordinator. Removed folders
/// have their index dropped (any outstanding `Index<Unlocked>` clones keep
/// the underlying `Arc` alive until they go out of scope).
pub async fn handle_did_change_workspace_folders(
    server: &RubyLanguageServer,
    params: DidChangeWorkspaceFoldersParams,
) {
    for removed in &params.event.removed {
        info!("Removing workspace folder: {}", removed.uri.as_str());
        server.remove_workspace(&removed.uri);
    }

    for added in params.event.added {
        info!("Adding workspace folder: {}", added.uri.as_str());
        let workspace = server.add_workspace(added.uri.clone());

        // Spawn coordinator for the new workspace. Mirrors the per-workspace
        // task spawned in `handle_initialized`, but for runtime additions.
        let server_clone = server.clone();
        let workspace_uri = workspace.root_uri.clone();
        let indexing_complete_flag = workspace.indexing_complete.clone();
        tokio::spawn(async move {
            info!(
                "Starting background indexing for newly added workspace: {}",
                workspace_uri.as_str()
            );
            match indexing::init_workspace(&server_clone, workspace_uri.clone()).await {
                Ok(_) => {
                    info!(
                        "Background indexing completed for added workspace: {}",
                        workspace_uri.as_str()
                    );
                    indexing_complete_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                Err(e) => {
                    warn!(
                        "Background indexing failed for added workspace {}: {}",
                        workspace_uri.as_str(),
                        e
                    );
                }
            }
        });
    }
}

pub async fn handle_did_change_configuration(
    server: &RubyLanguageServer,
    params: DidChangeConfigurationParams,
) {
    info!("Configuration change received");

    if let Some(settings) = params.settings.as_object() {
        if let Some(ruby_fast_lsp_settings) = settings.get("rubyFastLsp") {
            if let Ok(mut config) =
                serde_json::from_value::<RubyFastLspConfig>(ruby_fast_lsp_settings.clone())
            {
                preserve_initialization_only_config(
                    &mut config,
                    &server.config.lock(),
                    ruby_fast_lsp_settings,
                );
                info!("Updated configuration: {:?}", config);

                // Apply log level immediately (works without restart)
                config.apply_log_level();
                server.extension_registry.configure_from_config(&config);

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

fn preserve_initialization_only_config(
    config: &mut RubyFastLspConfig,
    current: &RubyFastLspConfig,
    settings: &serde_json::Value,
) {
    let Some(settings) = settings.as_object() else {
        return;
    };

    if !settings.contains_key("extensionPath") {
        config.extension_path = current.extension_path.clone();
    }
    if !settings.contains_key("extensionPackages") {
        config.extension_packages = current.extension_packages.clone();
    }
    if !settings.contains_key("extensionDirs") {
        config.extension_dirs = current.extension_dirs.clone();
    }
    if !settings.contains_key("extensionSettings") {
        config.extension_settings = current.extension_settings.clone();
    }
}

pub async fn handle_shutdown(_: &RubyLanguageServer) -> LspResult<()> {
    info!("Shutting down Ruby LSP server");
    Ok(())
}
