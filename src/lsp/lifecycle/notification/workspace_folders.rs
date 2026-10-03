//! Workspace folder additions and removals.

use super::configuration::refresh_extension_watch_registration;
use crate::lsp::lifecycle::indexing;
use crate::server::Server;
use log::{info, warn};
use tower_lsp::lsp_types::*;

/// Add or remove workspace folders at runtime in response to
/// `workspace/didChangeWorkspaceFolders`. Each added folder gets a freshly
/// spawned indexing coordinator.
pub async fn handle_did_change_workspace_folders(
    server: &Server,
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
        let owner = server.project_for_uri(&text_document.uri);
        server.remove_file_from_other_projects(&text_document.uri, &owner);
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
