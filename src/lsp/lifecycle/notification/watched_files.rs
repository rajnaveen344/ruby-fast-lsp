//! Watched-file changes, including project-input rebuilds of runtime-owned
//! state.

use crate::environment::config::runtime::EffectiveRuntimeSelection;
use crate::environment::config::RubyFastLspConfig;
use crate::environment::runtime::catalog::RuntimeImplementation;
use crate::lsp::lifecycle::indexing;
use crate::server::Server;
use log::{info, warn};
use tower_lsp::lsp_types::*;

pub async fn handle_did_change_watched_files(
    server: &Server,
    mut params: DidChangeWatchedFilesParams,
) {
    let debounce_generation = server.queue_watched_file_changes(params.changes);
    tokio::time::sleep(crate::server::WATCHED_FILE_DEBOUNCE_INTERVAL).await;
    let Some(changes) = server.take_watched_file_changes(debounce_generation) else {
        return;
    };
    params.changes = changes;

    let config = server.configuration_snapshot();
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
        if let Err(error) = server.reconfigure_extensions(&config).await {
            warn!("Project extension watcher reload failed: {error:#}");
        }
    }
    let workspace_trusted = server.with_configuration(|config| config.workspace_trusted);
    let reindex_uris = server
        .extension_registry()
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
    server.refresh_extension_watch_registration().await;
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

pub(super) fn project_input_change_requires_rebuild(
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

pub(super) async fn rebuild_runtime_owned_project_state(
    server: &Server,
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
        .open_documents()
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
    workspace.handle().reset();
    // The reset empties the engine in place, so its identity is unchanged.
    // Forget its seed after the reset, so no seed can land in between, or the
    // rebuild would skip seeding the empty engine.
    server
        .extension_registry()
        .forget_semantic_seed(&workspace.handle().load_target().engine_identity());
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
