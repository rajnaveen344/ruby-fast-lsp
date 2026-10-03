//! `initialize` (server capabilities and initialization options) and
//! `initialized` (client registrations and workspace indexing).

use crate::environment::config::RubyFastLspConfig;
use crate::environment::runtime::version::parse_ruby_family;
use crate::features::presentation::semantic_tokens;
use crate::lsp::lifecycle::indexing;
use crate::server::Server;
use log::{debug, info, warn};
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;

pub async fn handle_initialize(
    lang_server: &Server,
    params: InitializeParams,
) -> LspResult<InitializeResult> {
    let extension_watch_dynamic_registration = params
        .capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.did_change_watched_files.as_ref())
        .and_then(|watched_files| watched_files.dynamic_registration)
        .unwrap_or(false);
    lang_server.set_extension_watch_dynamic_registration(extension_watch_dynamic_registration);
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
    lang_server.replace_configuration(config.clone());

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
    if let Err(error) = lang_server.reconfigure_extensions(&config).await {
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
        inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(
            InlayHintOptions {
                work_done_progress_options: WorkDoneProgressOptions::default(),
                resolve_provider: Some(false),
            },
        ))),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            semantic_tokens::get_semantic_tokens_options(),
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
        document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
            first_trigger_character: "\n".to_string(),
            more_trigger_character: None,
        }),
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

pub async fn handle_initialized(server: &Server, _params: InitializedParams) {
    info!("Language server initialized");

    // Dynamically register type hierarchy capability (LSP 3.17.0)
    // lsp-types 0.94.1 doesn't have typeHierarchyProvider in ServerCapabilities,
    // so we use dynamic registration to enable the "Show Type Hierarchy" menu option.
    if let Some(client) = server.client() {
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

    server.refresh_extension_watch_registration().await;

    let config = server.configuration_snapshot();

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
