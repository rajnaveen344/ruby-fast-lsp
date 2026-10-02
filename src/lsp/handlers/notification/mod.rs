//! LSP Notification Handlers
//!
//! This module contains handlers for LSP notifications (events that don't require a response).
//! All helper functions and business logic should be in `helpers.rs`.

use crate::environment::config::runtime::EffectiveRuntimeSelection;
use crate::environment::config::RubyFastLspConfig;
use crate::environment::runtime::catalog::RuntimeImplementation;
use crate::environment::runtime::version::parse_ruby_family;
use crate::invariant::ExpectInvariant;
use crate::lsp::capabilities::{self, indexing};
use crate::server::RubyLanguageServer;
use log::{debug, info, warn};
use std::sync::atomic::Ordering;
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
        .extensions
        .dynamic_registration()
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
    if let Err(error) = lang_server
        .extensions
        .registry()
        .configure_from_config_and_workspace_roots_governed(
            &config,
            &lang_server.workspace_root_paths(),
            lang_server.indexing.resources().clone(),
        )
        .await
    {
        warn!("Extension initialization worker failed: {error:#}");
        return Err(tower_lsp::jsonrpc::Error::internal_error());
    }

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
            capabilities::presentation::inlay_hints::get_inlay_hints_capability(),
        )),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            capabilities::presentation::semantic_tokens::get_semantic_tokens_options(),
        )),
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(true),
            trigger_characters: Some(vec![
                ":".to_string(), // Trigger on ":" to handle "::" for constant completion
                ".".to_string(), // Trigger on "." for method completion
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
            capabilities::editing::formatting::get_document_on_type_formatting_options(),
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

    if let Some(version) = parse_ruby_family(&config.ruby_version) {
        info!("Using configured Ruby compatibility version: {version:?}");
    } else {
        info!("Ruby runtime and compatibility will be resolved independently per project");
    }

    // Spawn one coordinator per registered workspace. Coordinators feed the
    // shared analysis engine and only share the server for client notifications,
    // config, and document state.
    let workspaces = server.list_workspaces();
    if workspaces.is_empty() {
        info!("No workspaces registered; skipping background indexing");
        return;
    }

    let total = workspaces.len();

    let scheduled = workspaces
        .into_iter()
        .map(|ws| {
            let run = ws.begin_indexing_run();
            let admission = server.indexing.scheduler().register_cancellable(
                ws.root_path.clone(),
                crate::loader::scheduling::scheduler::IndexingPriority::Background,
                run.cancellation(),
            );
            (ws, run, admission)
        })
        .collect::<Vec<_>>();

    for (ws, run, admission) in scheduled {
        let server_clone = server.clone();
        tokio::spawn(async move {
            let workspace_uri = ws.root_uri.clone();
            server_clone.publish_indexing_status().await;
            let Some(_permit) = admission.wait().await else {
                return;
            };
            if ws
                .indexing_status
                .transition(
                    run.generation(),
                    crate::loader::scheduling::status::IndexingPhase::ResolvingRuntime,
                    None,
                    None,
                )
                .is_none()
            {
                return;
            }
            server_clone.publish_indexing_status().await;
            info!(
                "Starting background indexing for workspace: {}",
                workspace_uri.as_str()
            );

            let result =
                indexing::init_workspace_for_run(&server_clone, workspace_uri.clone(), run.clone())
                    .await;

            match result {
                Ok(_) => {
                    info!(
                        "Background indexing completed for workspace: {}",
                        workspace_uri.as_str()
                    );
                    ws.navigation_demands.complete_stage(
                        run.generation(),
                        crate::loader::scheduling::navigation_demand::NavigationDemandStage::Project,
                    );
                    ws.navigation_demands.complete_stage(
                        run.generation(),
                        crate::loader::scheduling::navigation_demand::NavigationDemandStage::Dependency,
                    );
                    let _ = ws.indexing_status.transition(
                        run.generation(),
                        crate::loader::scheduling::status::IndexingPhase::Ready,
                        None,
                        None,
                    );
                    server_clone.publish_indexing_status().await;
                }
                Err(e) => {
                    if run.is_cancelled() || !ws.indexing_status.is_current_run(&run) {
                        info!(
                            "Background indexing generation {} stopped for workspace {}: {}",
                            run.generation(),
                            workspace_uri.as_str(),
                            e
                        );
                        return;
                    }
                    ws.navigation_demands.cancel_generation(run.generation());
                    warn!(
                        "Background indexing failed for workspace {}: {}",
                        workspace_uri.as_str(),
                        e
                    );
                    let _ = ws.indexing_status.fail(run.generation(), e.to_string());
                    server_clone.publish_indexing_status().await;
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
    let debounce_generation = server.queue_watched_file_changes(params.changes);
    tokio::time::sleep(crate::server::WATCHED_FILE_DEBOUNCE_INTERVAL).await;
    let Some(changes) = server.take_watched_file_changes(debounce_generation) else {
        return;
    };
    params.changes = changes;

    let config = server.config.lock().clone();
    let workspaces = server.list_workspaces();
    let mut project_rebuilds = workspaces
        .iter()
        .filter(|workspace| {
            params.changes.iter().any(|change| {
                change.uri.to_file_path().is_ok_and(|path| {
                    project_input_change_requires_rebuild(&workspace.root_path, &path, &config)
                })
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    project_rebuilds.sort_by(|left, right| left.root_path.cmp(&right.root_path));
    project_rebuilds.dedup_by(|left, right| left.root_path == right.root_path);

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
    let extension_inputs_changed = config.workspace_trusted
        && params.changes.iter().any(|change| {
            change.uri.to_file_path().is_ok_and(|path| {
                workspaces
                    .iter()
                    .any(|workspace| project_extension_input_changed(&workspace.root_path, &path))
            })
        });
    if extension_inputs_changed {
        if let Err(error) = server
            .extensions
            .registry()
            .configure_from_config_and_workspace_roots_governed(
                &config,
                &server.workspace_root_paths(),
                server.indexing.resources().clone(),
            )
            .await
        {
            warn!("Project extension watcher reload failed: {error:#}");
        }
    }
    let workspace_trusted = server.config.lock().workspace_trusted;
    let reindex_uris = server
        .extensions
        .registry()
        .handle_watched_file_changes(
            workspace_trusted,
            &server.workspace_root_paths(),
            &params.changes,
            server.indexing.resources().clone(),
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
        !project_rebuilds.iter().any(|workspace| {
            project_input_change_requires_rebuild(&workspace.root_path, &path, &config)
        })
    });
    if !params.changes.is_empty() {
        indexing::handle_watched_files_changed(server, params).await;
    }
    for workspace in project_rebuilds {
        rebuild_runtime_owned_project_state(server, workspace).await;
    }
}

fn project_input_change_requires_rebuild(
    project_root: &std::path::Path,
    changed_path: &std::path::Path,
    config: &RubyFastLspConfig,
) -> bool {
    if !changed_path.starts_with(project_root) {
        return false;
    }
    let root = project_root.to_string_lossy();
    let file_name = changed_path.file_name().and_then(|name| name.to_str());
    if config.workspace_trusted && project_extension_input_changed(project_root, changed_path) {
        return true;
    }
    if matches!(file_name, Some("Gemfile" | "Gemfile.lock")) {
        return true;
    }
    let runtime_selection = config
        .runtime
        .selection_for_project(&root, &config.ruby_version);
    if changed_path.parent() == Some(project_root)
        && matches!(file_name, Some(".ruby-version" | ".tool-versions"))
    {
        return matches!(&runtime_selection, EffectiveRuntimeSelection::Auto);
    }
    if !matches!(
        &runtime_selection,
        EffectiveRuntimeSelection::Explicit(runtime)
            if runtime.implementation == RuntimeImplementation::Jruby
    ) {
        return false;
    }
    if matches!(file_name, Some("Jarfile" | "Jars.lock")) {
        return true;
    }
    changed_path
        .extension()
        .is_some_and(|extension| matches!(extension.to_str(), Some("jar" | "jmod" | "java")))
}

fn project_extension_input_changed(
    project_root: &std::path::Path,
    changed_path: &std::path::Path,
) -> bool {
    let Ok(relative) = changed_path.strip_prefix(project_root) else {
        return false;
    };
    let mut components = relative.components();
    match components
        .next()
        .and_then(|component| component.as_os_str().to_str())
    {
        Some(".ruby-fast-lsp") => components
            .next()
            .is_some_and(|component| component.as_os_str() == std::ffi::OsStr::new("extensions")),
        Some("ruby_fast_lsp") => true,
        Some(_) | None => false,
    }
}

async fn rebuild_runtime_owned_project_state(
    server: &RubyLanguageServer,
    workspace: crate::server::Workspace,
) {
    info!(
        "Rebuilding runtime-owned semantic state for project {}",
        workspace.root_path.display()
    );
    let run = workspace.begin_indexing_run();
    server.publish_indexing_status().await;
    let Some(_permit) = server
        .indexing
        .scheduler()
        .acquire_cancellable(
            workspace.root_path.clone(),
            crate::loader::scheduling::scheduler::IndexingPriority::OpenDocument,
            run.cancellation(),
        )
        .await
    else {
        return;
    };
    if workspace
        .indexing_status
        .transition(
            run.generation(),
            crate::loader::scheduling::status::IndexingPhase::ResolvingRuntime,
            None,
            None,
        )
        .is_none()
    {
        return;
    }
    server.publish_indexing_status().await;

    // The scheduler admits at most one generation per project. Only after the
    // superseded coordinator has released its permit may the replacement clear
    // and rebuild that project's semantic state.
    let open_documents = server
        .documents
        .read()
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
    server.release_external_documents_for_project(&workspace.root_uri);
    server.set_jruby_add_on(&workspace.root_path, None);
    server.set_effective_runtime(&workspace.root_path, None);
    server.set_extension_project_ruby_version(&workspace.root_path, None);
    *workspace.analysis_engine.write() = ruby_analysis::engine::AnalysisEngine::new();
    let rebuild =
        indexing::init_workspace_for_run(server, workspace.root_uri.clone(), run.clone()).await;
    for text_document in open_documents {
        indexing::handle_did_open(server, DidOpenTextDocumentParams { text_document }).await;
    }
    match rebuild {
        Ok(_) => {
            workspace.navigation_demands.complete_stage(
                run.generation(),
                crate::loader::scheduling::navigation_demand::NavigationDemandStage::Project,
            );
            workspace.navigation_demands.complete_stage(
                run.generation(),
                crate::loader::scheduling::navigation_demand::NavigationDemandStage::Dependency,
            );
            let _ = workspace.indexing_status.transition(
                run.generation(),
                crate::loader::scheduling::status::IndexingPhase::Ready,
                None,
                None,
            );
            server.publish_indexing_status().await;
        }
        Err(error) => {
            if run.is_cancelled() || !workspace.indexing_status.is_current_run(&run) {
                info!(
                    "Runtime rebuild generation {} stopped for project {}: {}",
                    run.generation(),
                    workspace.root_path.display(),
                    error
                );
                return;
            }
            workspace
                .navigation_demands
                .cancel_generation(run.generation());
            let _ = workspace
                .indexing_status
                .fail(run.generation(), error.to_string());
            server.publish_indexing_status().await;
            warn!(
                "Runtime rebuild failed for project {}: {error}",
                workspace.root_path.display()
            );
        }
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
        .documents
        .read()
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
    if let Err(error) = server
        .extensions
        .registry()
        .configure_from_config_and_workspace_roots_governed(
            &config,
            &server.workspace_root_paths(),
            server.indexing.resources().clone(),
        )
        .await
    {
        warn!("Extension workspace reconfiguration worker failed: {error:#}");
    }
    refresh_extension_watch_registration(server).await;

    for text_document in open_documents_to_rehome {
        let owner = server.analysis_engine_for_uri(&text_document.uri);
        server.remove_file_from_other_engines(&text_document.uri, &owner);
        indexing::handle_did_open(server, DidOpenTextDocumentParams { text_document }).await;
    }

    for workspace in added_workspaces {
        // Spawn coordinator for the new workspace. Mirrors the per-workspace
        // task spawned in `handle_initialized`, but only after extension
        // discovery includes the new root.
        let server_clone = server.clone();
        let workspace_uri = workspace.root_uri.clone();
        let project_root = workspace.root_path.clone();
        let run = workspace.begin_indexing_run();
        let indexing_status = workspace.indexing_status.clone();
        tokio::spawn(async move {
            server_clone.publish_indexing_status().await;
            let Some(_permit) = server_clone
                .indexing
                .scheduler()
                .acquire_cancellable(
                    project_root,
                    crate::loader::scheduling::scheduler::IndexingPriority::Background,
                    run.cancellation(),
                )
                .await
            else {
                return;
            };
            if indexing_status
                .transition(
                    run.generation(),
                    crate::loader::scheduling::status::IndexingPhase::ResolvingRuntime,
                    None,
                    None,
                )
                .is_none()
            {
                return;
            }
            server_clone.publish_indexing_status().await;
            info!(
                "Starting background indexing for newly added workspace: {}",
                workspace_uri.as_str()
            );
            match indexing::init_workspace_for_run(
                &server_clone,
                workspace_uri.clone(),
                run.clone(),
            )
            .await
            {
                Ok(_) => {
                    info!(
                        "Background indexing completed for added workspace: {}",
                        workspace_uri.as_str()
                    );
                    workspace.navigation_demands.complete_stage(
                        run.generation(),
                        crate::loader::scheduling::navigation_demand::NavigationDemandStage::Project,
                    );
                    workspace.navigation_demands.complete_stage(
                        run.generation(),
                        crate::loader::scheduling::navigation_demand::NavigationDemandStage::Dependency,
                    );
                    let _ = indexing_status.transition(
                        run.generation(),
                        crate::loader::scheduling::status::IndexingPhase::Ready,
                        None,
                        None,
                    );
                    server_clone.publish_indexing_status().await;
                }
                Err(e) => {
                    if run.is_cancelled() || !indexing_status.is_current_run(&run) {
                        info!(
                            "Added-workspace indexing generation {} stopped for {}: {}",
                            run.generation(),
                            workspace_uri.as_str(),
                            e
                        );
                        return;
                    }
                    workspace
                        .navigation_demands
                        .cancel_generation(run.generation());
                    warn!(
                        "Background indexing failed for added workspace {}: {}",
                        workspace_uri.as_str(),
                        e
                    );
                    let _ = indexing_status.fail(run.generation(), e.to_string());
                    server_clone.publish_indexing_status().await;
                }
            }
        });
    }
    server.publish_indexing_status().await;
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
                if let Err(error) = server
                    .extensions
                    .registry()
                    .configure_from_config_and_workspace_roots_governed(
                        &config,
                        &server.workspace_root_paths(),
                        server.indexing.resources().clone(),
                    )
                    .await
                {
                    warn!("Extension settings reconfiguration worker failed: {error:#}");
                    return;
                }
                refresh_extension_watch_registration(server).await;

                *server.config.lock() = config.clone();

                if let Some(version) = parse_ruby_family(&config.ruby_version) {
                    info!("Using configured Ruby compatibility version: {version:?}");
                } else {
                    info!(
                        "Ruby runtime and compatibility will be resolved independently per project"
                    );
                }

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
        .extensions
        .dynamic_registration()
        .load(Ordering::Acquire)
    {
        return;
    }
    let Some(client) = &server.client else {
        return;
    };

    let desired = server.extensions.registry().watcher_globs();
    let mut current = server.extensions.registration().lock().await;
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
        register_options: Some(serde_json::to_value(options).expect_invariant(
            "typed watched-file registration options failed to serialize",
            "lsp-types registration values must serialize",
            "preserve serializable watcher option fields",
        )),
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
    server.cancel_watched_file_changes();
    server.cancel_all_indexing();
    server.extensions.registry().shutdown();
    Ok(())
}

#[cfg(test)]
mod tests;
