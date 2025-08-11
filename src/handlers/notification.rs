use std::sync::Arc;

use crate::capabilities;
use crate::config::RubyFastLspConfig;
use crate::handlers::helpers::{
    init_workspace, process_file_for_definitions, process_file_for_references,
};
use crate::server::RubyLanguageServer;
use crate::version::MinorVersion;
use crate::types::ruby_document::RubyDocument;
use log::{debug, info, warn};
use tower_lsp::lsp_types::{
    DidChangeConfigurationParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, InitializeParams, InitializedParams, *
};
use parking_lot::RwLock;
use tower_lsp::jsonrpc::Result as LspResult;

pub async fn handle_initialize(
    lang_server: &RubyLanguageServer,
    params: InitializeParams,
) -> LspResult<InitializeResult> {
    let workspace_folders = params.workspace_folders;

    // Process initialization options for configuration
    if let Some(init_options) = params.initialization_options {
        if let Ok(config) = serde_json::from_value::<RubyFastLspConfig>(init_options) {
            info!("Received configuration: {:?}", config);
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
                ":".to_string(),  // Trigger on ":" to handle "::" for constant completion
                ".".to_string(),  // Trigger on "." for method completion (future enhancement)
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

pub async fn handle_initialized(
    server: &RubyLanguageServer,
    _params: InitializedParams,
) {
    info!("Language server initialized");

    let config = server.config.lock().clone();
    
    // Determine Ruby version based on configuration
    let ruby_version = if let Some(version) = config.get_ruby_version() {
        info!("Using configured Ruby version: {:?}", version);
        version
    } else {
        // Auto-detect Ruby version
        detect_system_ruby_version()
            .unwrap_or_else(|| {
                info!("No Ruby version detected, using default Ruby 3.0");
                (3, 0)
            })
    };

    info!("Using Ruby version: {:?}", ruby_version);

    // Core stubs will be handled by indexing additional paths
    if config.enable_core_stubs {
        info!("Core stubs enabled - will be indexed from VSIX stubs directory");
    } else {
        info!("Core stubs disabled in configuration");
    }
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
                if let Some(version) = MinorVersion::from_full_version(version_part) {
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

    let document = RubyDocument::new(
        uri.clone(),
        content.clone(),
        params.text_document.version,
    );

    lang_server.docs.lock().insert(
        uri.clone(),
        Arc::new(RwLock::new(document.clone())),
    );
    debug!("Doc cache size: {}", lang_server.docs.lock().len());

    let _ = process_file_for_definitions(lang_server, uri.clone());
    let _ = process_file_for_references(lang_server, uri.clone(), true);
    
    // Generate and publish diagnostics
    let diagnostics = capabilities::diagnostics::generate_diagnostics(&document);
    lang_server.publish_diagnostics(uri, diagnostics).await;
}

pub async fn handle_did_change(
    lang_server: &RubyLanguageServer,
    params: DidChangeTextDocumentParams,
) {
    let uri = params.text_document.uri.clone();

    for change in params.content_changes {
        let content = change.text.clone();
        let doc = RubyDocument::new(uri.clone(), content.clone(), params.text_document.version);

        lang_server
            .docs
            .lock()
            .insert(uri.clone(), Arc::new(RwLock::new(doc.clone())));

        let _ = process_file_for_definitions(lang_server, uri.clone());
        let _ = process_file_for_references(lang_server, uri.clone(), true);
        
        // Generate and publish diagnostics
        let diagnostics = capabilities::diagnostics::generate_diagnostics(&doc);
        lang_server.publish_diagnostics(uri.clone(), diagnostics).await;
    }
}

pub async fn handle_did_close(
    lang_server: &RubyLanguageServer,
    params: DidCloseTextDocumentParams,
) {
    let uri = params.text_document.uri.clone();
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
            if let Ok(config) = serde_json::from_value::<RubyFastLspConfig>(ruby_fast_lsp_settings.clone()) {
                info!("Updated configuration: {:?}", config);
                
                // Update the server configuration
                *server.config.lock() = config.clone();
                
                // Handle configuration change for Ruby version
                let ruby_version = if let Some(version) = config.get_ruby_version() {
                    info!("Using configured Ruby version: {:?}", version);
                    version
                } else {
                    // Auto-detect Ruby version
                    detect_system_ruby_version()
                        .unwrap_or_else(|| {
                            info!("No Ruby version detected, using default Ruby 3.0");
                            (3, 0)
                        })
                };

                info!("Configuration updated with Ruby version: {:?}", ruby_version);

                // Core stubs will be handled by indexing additional paths
                if config.enable_core_stubs {
                    info!("Core stubs enabled - will be indexed from VSIX stubs directory");
                } else {
                    info!("Core stubs disabled in updated configuration");
                }
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
