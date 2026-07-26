//! LSP Notification Handlers
//!
//! This module contains handlers for LSP notifications (events that don't require a response).
//! All helper functions and business logic should be in `helpers.rs`.

use crate::capabilities::{self, indexing};
use crate::config::runtime::EffectiveRuntimeSelection;
use crate::config::RubyFastLspConfig;
use crate::runtime::catalog::RuntimeImplementation;
use crate::server::RubyLanguageServer;
use crate::utils::detect_system_ruby_version;
use log::{debug, info, warn};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;

pub async fn handle_initialize(
    lang_server: &RubyLanguageServer,
    params: InitializeParams,
) -> LspResult<InitializeResult> {
    let extension_watch_dynamic_registration = params
        .capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.did_change_watched_files.as_ref())
        .and_then(|watched_files| watched_files.dynamic_registration)
        .unwrap_or(false);
    lang_server
        .extension_watch_dynamic_registration
        .store(extension_watch_dynamic_registration, Ordering::Release);
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

    // Parse configuration before workspace registration, then configure
    // extensions after roots are known so trusted project-local packages can
    // participate in deterministic discovery.
    let config = match params.initialization_options {
        Some(init_options) => match serde_json::from_value::<RubyFastLspConfig>(init_options) {
            Ok(config) if config.validate_runtime_configuration().is_ok() => {
                debug!("Received configuration: {:?}", config);
                config
            }
            Ok(config) => {
                warn!(
                    "Rejected invalid runtime initialization configuration: {}",
                    config.validate_runtime_configuration().expect_err(
                        "invalid configuration branch must retain its validation error"
                    )
                );
                RubyFastLspConfig::default()
            }
            Err(err) => {
                warn!(
                    "Failed to parse initialization options as configuration: {}",
                    err
                );
                RubyFastLspConfig::default()
            }
        },
        None => RubyFastLspConfig::default(),
    };
    *lang_server.config.lock() = config.clone();

    // Register every workspace folder. Each folder is indexed independently
    // in handle_initialized. Multi-root VS Code
    // workspaces, Solargraph-style — folders do not bleed into one another.
    let folders: Vec<WorkspaceFolder> = workspace_folders.unwrap_or_default();
    if !folders.is_empty() {
        for folder in &folders {
            info!(
                "Registering workspace folder for indexing: {}",
                folder.uri.as_str()
            );
            if let Err(error) = lang_server.add_workspace_folder(folder.uri.clone()) {
                warn!(
                    "Failed to discover Ruby projects in workspace folder {}: {}",
                    folder.uri.as_str(),
                    error
                );
            }
        }
    } else if let Some(root) = root_uri {
        info!("Registering workspace root for indexing: {}", root.as_str());
        if let Err(error) = lang_server.add_workspace_folder(root.clone()) {
            warn!(
                "Failed to discover Ruby projects in workspace root {}: {}",
                root.as_str(),
                error
            );
        }
    } else {
        warn!("No workspace folder or root URI provided. Files opened ad-hoc will use the orphan index.");
    }
    lang_server
        .extension_registry
        .configure_from_config_and_workspace_roots(&config, &lang_server.workspace_root_paths());

    // Build static capabilities
    // Note: Type hierarchy is dynamically registered in handle_initialized
    // because lsp-types 0.94.1 doesn't have typeHierarchyProvider field
    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        definition_provider: Some(OneOf::Left(true)),
        implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
        references_provider: Some(OneOf::Left(true)),
        document_highlight_provider: Some(OneOf::Left(true)),
        selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
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
        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
            retrigger_characters: Some(vec![",".to_string()]),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        }),
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
            resolve_provider: Some(false),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),
        document_on_type_formatting_provider: Some(
            capabilities::formatting::get_document_on_type_formatting_options(),
        ),
        document_formatting_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),
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

    refresh_extension_watch_registration(server).await;

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

    // Spawn one coordinator per registered workspace. Coordinators feed the
    // shared analysis engine and only share the server for client notifications,
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
    mut params: DidChangeWatchedFilesParams,
) {
    let config = server.config.lock().clone();
    let mut runtime_rebuilds = server
        .list_workspaces()
        .into_iter()
        .filter(|workspace| {
            params.changes.iter().any(|change| {
                change.uri.to_file_path().is_ok_and(|path| {
                    jruby_classpath_change_requires_rebuild(&workspace.root_path, &path, &config)
                })
            })
        })
        .collect::<Vec<_>>();
    runtime_rebuilds.sort_by(|left, right| left.root_path.cmp(&right.root_path));
    runtime_rebuilds.dedup_by(|left, right| left.root_path == right.root_path);

    for change in &params.changes {
        if change
            .uri
            .to_file_path()
            .ok()
            .and_then(|path| path.file_name().map(|name| name == "Gemfile.lock"))
            .unwrap_or(false)
        {
            server.refresh_extension_project_dependencies_for_uri(&change.uri);
        }
    }
    let workspace_trusted = server.config.lock().workspace_trusted;
    let reindex_uris = server
        .extension_registry
        .handle_watched_file_changes(
            workspace_trusted,
            &server.workspace_root_paths(),
            &params.changes,
        )
        .await;
    params
        .changes
        .extend(reindex_uris.into_iter().map(|uri| FileEvent {
            uri,
            typ: FileChangeType::CHANGED,
        }));
    refresh_extension_watch_registration(server).await;
    params.changes.retain(|change| {
        let Ok(path) = change.uri.to_file_path() else {
            return true;
        };
        !runtime_rebuilds.iter().any(|workspace| {
            jruby_classpath_change_requires_rebuild(&workspace.root_path, &path, &config)
        })
    });
    if !params.changes.is_empty() {
        indexing::handle_watched_files_changed(server, params).await;
    }
    for workspace in runtime_rebuilds {
        rebuild_runtime_owned_project_state(server, workspace).await;
    }
}

fn jruby_classpath_change_requires_rebuild(
    project_root: &std::path::Path,
    changed_path: &std::path::Path,
    config: &RubyFastLspConfig,
) -> bool {
    if !changed_path.starts_with(project_root) {
        return false;
    }
    let root = project_root.to_string_lossy();
    if !matches!(
        config
            .runtime
            .selection_for_project(&root, &config.ruby_version),
        EffectiveRuntimeSelection::Explicit(runtime)
            if runtime.implementation == RuntimeImplementation::Jruby
    ) {
        return false;
    }
    if changed_path.file_name().is_some_and(|name| {
        matches!(
            name.to_str(),
            Some("Gemfile.lock" | "Jarfile" | "Jars.lock")
        )
    }) {
        return true;
    }
    changed_path
        .extension()
        .is_some_and(|extension| matches!(extension.to_str(), Some("jar" | "jmod" | "java")))
}

async fn rebuild_runtime_owned_project_state(
    server: &RubyLanguageServer,
    workspace: crate::server::Workspace,
) {
    let open_documents = server
        .docs
        .lock()
        .values()
        .filter_map(|document| {
            let document = document.read();
            let path = document.uri.to_file_path().ok()?;
            path.starts_with(&workspace.root_path)
                .then(|| TextDocumentItem {
                    uri: document.uri.clone(),
                    language_id: "ruby".to_string(),
                    version: document.version,
                    text: document.content.clone(),
                })
        })
        .collect::<Vec<_>>();
    info!(
        "Rebuilding runtime-owned semantic state for project {}",
        workspace.root_path.display()
    );
    server.release_external_documents_for_project(&workspace.root_uri);
    *workspace.analysis_engine.write() = ruby_analysis::engine::AnalysisEngine::new();
    workspace.indexing_complete.store(false, Ordering::Release);
    let rebuild = indexing::init_workspace(server, workspace.root_uri.clone()).await;
    for text_document in open_documents {
        indexing::handle_did_open(server, DidOpenTextDocumentParams { text_document }).await;
    }
    match rebuild {
        Ok(()) => {
            workspace.indexing_complete.store(true, Ordering::Release);
        }
        Err(error) => warn!(
            "Runtime rebuild failed for project {}: {error}",
            workspace.root_path.display()
        ),
    }
}

/// Add or remove workspace folders at runtime in response to
/// `workspace/didChangeWorkspaceFolders`. Each added folder gets a freshly
/// spawned indexing coordinator.
pub async fn handle_did_change_workspace_folders(
    server: &RubyLanguageServer,
    params: DidChangeWorkspaceFoldersParams,
) {
    let changed_paths = params
        .event
        .removed
        .iter()
        .chain(params.event.added.iter())
        .filter_map(|folder| folder.uri.to_file_path().ok())
        .collect::<Vec<_>>();
    let open_documents_to_rehome = server
        .docs
        .lock()
        .values()
        .filter_map(|document| {
            let document = document.read();
            let path = document.uri.to_file_path().ok()?;
            changed_paths
                .iter()
                .any(|removed| path.starts_with(removed))
                .then(|| TextDocumentItem {
                    uri: document.uri.clone(),
                    language_id: "ruby".to_string(),
                    version: document.version,
                    text: document.content.clone(),
                })
        })
        .collect::<Vec<_>>();

    for removed in &params.event.removed {
        info!("Removing workspace folder: {}", removed.uri.as_str());
        server.remove_workspace_folder(&removed.uri);
    }

    let mut added_workspaces = Vec::new();
    for added in params.event.added {
        info!("Adding workspace folder: {}", added.uri.as_str());
        match server.add_workspace_folder(added.uri.clone()) {
            Ok(projects) => added_workspaces.extend(projects),
            Err(error) => warn!(
                "Failed to discover Ruby projects in workspace folder {}: {}",
                added.uri.as_str(),
                error
            ),
        }
    }

    let config = server.config.lock().clone();
    server
        .extension_registry
        .configure_from_config_and_workspace_roots(&config, &server.workspace_root_paths());
    refresh_extension_watch_registration(server).await;

    for text_document in open_documents_to_rehome {
        let owner = server.analysis_engine_for_uri(&text_document.uri);
        server.clear_file_from_other_engines(&text_document.uri, &owner);
        indexing::handle_did_open(server, DidOpenTextDocumentParams { text_document }).await;
    }

    for workspace in added_workspaces {
        // Spawn coordinator for the new workspace. Mirrors the per-workspace
        // task spawned in `handle_initialized`, but only after extension
        // discovery includes the new root.
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
                if let Err(error) = config.validate_runtime_configuration() {
                    warn!("Rejected invalid runtime configuration update: {error}");
                    return;
                }
                let previous_config = server.config.lock().clone();
                preserve_initialization_only_config(
                    &mut config,
                    &previous_config,
                    ruby_fast_lsp_settings,
                );
                let runtime_changed_workspaces = server
                    .list_workspaces()
                    .into_iter()
                    .filter(|workspace| {
                        let root = workspace.root_path.to_string_lossy();
                        previous_config
                            .runtime
                            .selection_for_project(&root, &previous_config.ruby_version)
                            != config
                                .runtime
                                .selection_for_project(&root, &config.ruby_version)
                            || previous_config.jruby.project_config(&root)
                                != config.jruby.project_config(&root)
                    })
                    .collect::<Vec<_>>();
                info!("Updated configuration: {:?}", config);

                // Apply log level immediately (works without restart)
                config.apply_log_level();
                server
                    .extension_registry
                    .configure_from_config_and_workspace_roots(
                        &config,
                        &server.workspace_root_paths(),
                    );
                refresh_extension_watch_registration(server).await;

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

                for workspace in runtime_changed_workspaces {
                    rebuild_runtime_owned_project_state(server, workspace).await;
                }
            } else {
                warn!("Failed to parse configuration from settings");
            }
        }
    }
}

async fn refresh_extension_watch_registration(server: &RubyLanguageServer) {
    if !server
        .extension_watch_dynamic_registration
        .load(Ordering::Acquire)
    {
        return;
    }
    let Some(client) = &server.client else {
        return;
    };

    let desired = server.extension_registry.watcher_globs();
    let mut current = server.extension_watch_registration.lock().await;
    if *current == desired {
        return;
    }

    if !current.is_empty() {
        let unregistration = Unregistration {
            id: "ruby-fast-lsp-extension-watchers".to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
        };
        if let Err(err) = client.unregister_capability(vec![unregistration]).await {
            warn!("Failed to unregister extension file watchers: {:?}", err);
            return;
        }
        current.clear();
    }

    if desired.is_empty() {
        return;
    }
    let registration = extension_watch_registration(&desired);
    match client.register_capability(vec![registration]).await {
        Ok(()) => {
            info!(
                "Registered {} extension watched-file glob(s)",
                desired.len()
            );
            *current = desired;
        }
        Err(err) => warn!("Failed to register extension file watchers: {:?}", err),
    }
}

fn extension_watch_registration(globs: &[String]) -> Registration {
    let options = DidChangeWatchedFilesRegistrationOptions {
        watchers: globs
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|pattern| FileSystemWatcher {
                glob_pattern: GlobPattern::String(pattern),
                kind: None,
            })
            .collect(),
    };
    Registration {
        id: "ruby-fast-lsp-extension-watchers".to_string(),
        method: "workspace/didChangeWatchedFiles".to_string(),
        register_options: Some(
            serde_json::to_value(options).expect(
                "INVARIANT VIOLATED: typed watched-file registration options failed to serialize. This is a bug because lsp-types registration values must serialize. Fix: preserve serializable watcher option fields.",
            ),
        ),
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
    if !settings.contains_key("workspaceTrusted") {
        config.workspace_trusted = current.workspace_trusted;
    }
    if !settings.contains_key("projectExtensionsEnabled") {
        config.project_extensions_enabled = current.project_extensions_enabled;
    }
}

pub async fn handle_shutdown(server: &RubyLanguageServer) -> LspResult<()> {
    info!("Shutting down Ruby LSP server");
    server.extension_registry.shutdown();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::runtime::{
        ProjectRuntimeSelection, RuntimeMode, RuntimeSelection, RuntimeSelectionConfig,
        SelectedRuntimeDescriptor,
    };
    use crate::runtime::catalog::RuntimeDiscoverySource;
    use ruby_analysis::core::SourceKind;
    use ruby_analysis::engine::SourceFileInput;
    use std::io::{Cursor, Write};
    use std::path::PathBuf;
    use zip::write::SimpleFileOptions;

    fn decode_hex(source: &str) -> Vec<u8> {
        let digits = source
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        digits
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    fn write_jar(path: &std::path::Path, entry: &str, contents: &[u8]) {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(entry, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(contents).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn extension_watch_registration_is_sorted_typed_lsp_registration() {
        let registration = extension_watch_registration(&[
            "config/**/*.yml".to_string(),
            ".rubocop.yml".to_string(),
            "config/**/*.yml".to_string(),
        ]);

        assert_eq!(registration.id, "ruby-fast-lsp-extension-watchers");
        assert_eq!(registration.method, "workspace/didChangeWatchedFiles");
        assert_eq!(
            registration.register_options,
            Some(serde_json::json!({
                "watchers": [
                    {"globPattern": ".rubocop.yml"},
                    {"globPattern": "config/**/*.yml"}
                ]
            }))
        );
    }

    #[test]
    fn only_owning_jruby_classpath_inputs_trigger_runtime_rebuilds() {
        let project = PathBuf::from("/repo/admin");
        let mut config = RubyFastLspConfig {
            runtime: RuntimeSelectionConfig {
                mode: RuntimeMode::Auto,
                projects: vec![ProjectRuntimeSelection {
                    root: project.to_string_lossy().to_string(),
                    selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                        implementation: RuntimeImplementation::Jruby,
                        family: "9.2".to_string(),
                        engine_version: "9.2.21.0".to_string(),
                        compatibility_version: "2.5".to_string(),
                        executable: PathBuf::from("/runtimes/jruby-9.2.21.0/bin/jruby"),
                        discovery_source: RuntimeDiscoverySource::Rvm,
                        java_home: Some(PathBuf::from("/jdk/17")),
                    }),
                }],
            },
            ..RubyFastLspConfig::default()
        };

        for changed in [
            "Gemfile.lock",
            "Jarfile",
            "Jars.lock",
            "lib/jars/runtime.jar",
            "src/main/java/com/example/Runtime.java",
        ] {
            assert!(
                jruby_classpath_change_requires_rebuild(&project, &project.join(changed), &config),
                "{changed} must rebuild the owning JRuby runtime state"
            );
        }
        assert!(!jruby_classpath_change_requires_rebuild(
            &project,
            PathBuf::from("/repo/server/lib/jars/runtime.jar").as_path(),
            &config
        ));
        assert!(!jruby_classpath_change_requires_rebuild(
            &project,
            &project.join("lib/application.rb"),
            &config
        ));

        config.runtime.projects[0].selection =
            RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                implementation: RuntimeImplementation::Mri,
                family: "3.3".to_string(),
                engine_version: "3.3.11".to_string(),
                compatibility_version: "3.3".to_string(),
                executable: PathBuf::from("/runtimes/ruby-3.3.11/bin/ruby"),
                discovery_source: RuntimeDiscoverySource::Rvm,
                java_home: None,
            });
        assert!(!jruby_classpath_change_requires_rebuild(
            &project,
            &project.join("lib/jars/runtime.jar"),
            &config
        ));
    }

    #[tokio::test]
    async fn classpath_change_clears_external_facts_and_reopens_project_documents_on_failure() {
        let fixture = tempfile::tempdir().unwrap();
        let project = fixture.path().join("admin");
        std::fs::create_dir_all(project.join("lib/jars")).unwrap();
        std::fs::write(project.join("Gemfile"), "").unwrap();
        let source_path = project.join("app.rb");
        let source = "VALUE = Java::ComExample::Runtime.new\n";
        std::fs::write(&source_path, source).unwrap();
        let project_uri = Url::from_directory_path(&project).unwrap();
        let source_uri = Url::from_file_path(&source_path).unwrap();
        let external_path = fixture.path().join("cache/Runtime.java");
        std::fs::create_dir_all(external_path.parent().unwrap()).unwrap();
        std::fs::write(&external_path, "package com.example; class Runtime {}\n").unwrap();
        let external_uri = Url::from_file_path(&external_path).unwrap();

        let server = RubyLanguageServer::default();
        let workspace = server.add_workspace(project_uri);
        *server.config.lock() = RubyFastLspConfig {
            runtime: RuntimeSelectionConfig {
                mode: RuntimeMode::Auto,
                projects: vec![ProjectRuntimeSelection {
                    root: project.to_string_lossy().to_string(),
                    selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                        implementation: RuntimeImplementation::Jruby,
                        family: "9.2".to_string(),
                        engine_version: "9.2.21.0".to_string(),
                        compatibility_version: "2.5".to_string(),
                        executable: fixture.path().join("missing-jruby/bin/jruby"),
                        discovery_source: RuntimeDiscoverySource::Rvm,
                        java_home: Some(fixture.path().join("missing-jdk")),
                    }),
                }],
            },
            ..RubyFastLspConfig::default()
        };
        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: source_uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: source.to_string(),
                },
            },
        )
        .await;
        workspace
            .analysis_engine
            .write()
            .register_file(SourceFileInput {
                path: external_path.clone(),
                content: "package com.example; class Runtime {}\n".to_string(),
                kind: SourceKind::External,
            });
        server.retain_external_document_project(&external_uri, &workspace);
        assert!(workspace
            .analysis_engine
            .read()
            .file_id(&external_path)
            .is_some());

        handle_did_change_watched_files(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: Url::from_file_path(project.join("lib/jars/runtime.jar")).unwrap(),
                    typ: FileChangeType::DELETED,
                }],
            },
        )
        .await;

        let engine = workspace.analysis_engine.read();
        assert!(
            engine.file_id(&external_path).is_none(),
            "runtime rebuild must remove stale external implementation facts"
        );
        assert!(
            engine.file_id(&source_path).is_some(),
            "open project documents must be restored even when runtime setup fails closed"
        );
        drop(engine);
        assert!(
            server.analysis_workspace_for_uri(&external_uri).is_none(),
            "runtime rebuild must release retained provenance for stale external documents"
        );
        assert!(!workspace.indexing_complete.load(Ordering::Acquire));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn changed_winning_jar_replaces_decompiled_navigation_without_stale_facts() {
        use crate::config::runtime::ProjectJrubyConfig;
        use std::os::unix::fs::symlink;

        let fixture = tempfile::tempdir().unwrap();
        let project = fixture.path().join("admin");
        let jruby_home = fixture.path().join("jruby-9.2.21.0");
        let java_home = fixture.path().join("jdk");
        let jar_path = project.join("lib/rich.jar");
        let source_path = project.join("imports.rb");
        std::fs::create_dir_all(project.join("lib")).unwrap();
        std::fs::create_dir_all(jruby_home.join("bin")).unwrap();
        std::fs::create_dir_all(java_home.join("bin")).unwrap();
        std::fs::create_dir_all(java_home.join("jmods")).unwrap();
        std::fs::write(project.join("Gemfile"), "").unwrap();
        std::fs::write(jruby_home.join("bin/jruby"), "#!/bin/sh\nprintf '[]\\n'\n").unwrap();
        let permissions = std::os::unix::fs::PermissionsExt::from_mode(0o755);
        std::fs::set_permissions(jruby_home.join("bin/jruby"), permissions).unwrap();
        std::fs::write(java_home.join("release"), "JAVA_VERSION=\"17.0.12\"\n").unwrap();
        let real_java = [
            std::env::var_os("JAVA_HOME")
                .map(PathBuf::from)
                .map(|home| home.join("bin/java")),
            Some(PathBuf::from("/opt/homebrew/opt/openjdk/bin/java")),
            Some(PathBuf::from("/usr/local/opt/openjdk/bin/java")),
        ]
        .into_iter()
        .flatten()
        .find(|candidate| candidate.is_file())
        .expect("JRuby decompiler lifecycle test requires a real JDK java executable");
        symlink(real_java, java_home.join("bin/java")).unwrap();
        let rich_class = decode_hex(include_str!(
            "../../crates/jvm-metadata/fixtures/rich_fixture.class.hex"
        ));
        write_jar(&jar_path, "fixtures/RichFixture.class", &rich_class);
        let source = "java_import fixtures.RichFixture\n\
                      RICH = RichFixture.new(nil)\n\
                      VALUE = RICH.java_send(:run, [])\n";
        std::fs::write(&source_path, source).unwrap();

        let project_root = format!("{}/", project.to_string_lossy());
        let mut config = RubyFastLspConfig {
            runtime: RuntimeSelectionConfig {
                mode: RuntimeMode::Auto,
                projects: vec![ProjectRuntimeSelection {
                    root: project_root.clone(),
                    selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                        implementation: RuntimeImplementation::Jruby,
                        family: "9.2".to_string(),
                        engine_version: "9.2.21.0".to_string(),
                        compatibility_version: "2.5".to_string(),
                        executable: jruby_home.join("bin/jruby"),
                        discovery_source: RuntimeDiscoverySource::Rvm,
                        java_home: Some(java_home),
                    }),
                }],
            },
            ..RubyFastLspConfig::default()
        };
        config.jruby.projects = vec![ProjectJrubyConfig {
            root: project_root,
            additional_classpath: vec!["lib/rich.jar".to_string()],
            additional_sources: Vec::new(),
        }];

        let server = RubyLanguageServer::default();
        *server.config.lock() = config;
        server.set_user_cache_root_for_tests(fixture.path().join("cache"));
        let project_uri = Url::from_directory_path(&project).unwrap();
        let workspace = server.add_workspace(project_uri.clone());
        indexing::init_workspace(&server, project_uri)
            .await
            .expect("initial JRuby fixture workspace must index");
        let initial_files = workspace
            .analysis_engine
            .read()
            .files()
            .map(|file| (file.kind, file.path.clone()))
            .collect::<Vec<_>>();
        assert!(
            initial_files.iter().any(|(kind, path)| {
                *kind == SourceKind::External
                    && std::fs::read_to_string(path).is_ok_and(|source| {
                        source.contains("return List.of(prefix + values.length);")
                    })
            }),
            "initial runtime index must contain the decompiled implementation: {initial_files:?}"
        );

        let replacement = decode_hex(include_str!(
            "../../crates/jvm-metadata/fixtures/minimal_class.hex"
        ));
        write_jar(&jar_path, "com/example/Demo.class", &replacement);
        handle_did_change_watched_files(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: Url::from_file_path(&jar_path).unwrap(),
                    typ: FileChangeType::CHANGED,
                }],
            },
        )
        .await;

        let engine = workspace.analysis_engine.read();
        assert!(
            !engine.files().any(|file| {
                matches!(file.kind, SourceKind::External | SourceKind::Signature)
                    && file.path.to_string_lossy().contains("RichFixture")
            }),
            "changing the winning artifact must remove stale source, decompiled, and signature facts"
        );
        assert!(
            engine.file_id(&source_path).is_some(),
            "the project source must be reindexed after the runtime rebuild"
        );
        assert!(workspace.indexing_complete.load(Ordering::Acquire));
    }
}
