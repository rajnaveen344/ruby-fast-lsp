use crate::capabilities::diagnostics::generate_diagnostics;
use crate::indexer::coordinator::IndexingCoordinator;
use crate::indexer::file_processor::FileProcessor;
use crate::linter::lint_document;
use crate::query::EngineQuery;
use crate::server::RubyLanguageServer;
use crate::utils::ProjectFilePolicy;
use ruby_analysis::core::SourceKind;
use ruby_analysis::engine::{FileFacts, ResolveMode, SourceFileInput};
use ruby_analysis::indexer::RubyDocument;

use log::{debug, info};
use parking_lot::RwLock;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::*;

const MAX_OPEN_DIAGNOSTIC_REFRESH_FILES: usize = 8;
const INTERACTIVE_SEMANTIC_TRANSIENT_MEMORY_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Copy)]
enum DocumentSemanticMode {
    CurrentFile,
    Full,
}

fn interactive_file_processor(server: &RubyLanguageServer, uri: &Url) -> FileProcessor {
    let processor = FileProcessor::with_extension_registry(server.extension_registry.clone());
    server
        .jruby_import_provider_for_uri(uri)
        .map(|provider| processor.clone().with_jruby_import_provider(provider))
        .unwrap_or(processor)
}

async fn process_interactive_file(
    indexer: &FileProcessor,
    server: &RubyLanguageServer,
    uri: &Url,
    content: &str,
    mode: DocumentSemanticMode,
) -> anyhow::Result<crate::indexer::file_processor::ProcessResult> {
    let workspace = server.workspace_for_uri(uri);
    let project_root = workspace
        .as_ref()
        .map(|workspace| workspace.root_path.clone())
        .or_else(|| {
            uri.to_file_path()
                .ok()
                .and_then(|path| path.parent().map(Path::to_path_buf))
        });
    let current_file_resolution =
        matches!(mode, DocumentSemanticMode::CurrentFile) && workspace.is_some();
    let mode_label = match (mode, workspace.is_some()) {
        (DocumentSemanticMode::CurrentFile, true) => "current-file",
        (DocumentSemanticMode::CurrentFile, false) => "full-orphan",
        (DocumentSemanticMode::Full, _) => "full",
    };
    let indexer = indexer.clone();
    let server = server.clone();
    let uri = uri.clone();
    let content = content.to_string();
    let spec = crate::indexing_resources::IndexingWorkSpec::new(
        project_root,
        crate::indexing_resources::IndexingResourcePriority::OpenDocument,
        1,
        INTERACTIVE_SEMANTIC_TRANSIENT_MEMORY_BYTES,
        1,
    );
    server
        .indexing_resources
        .clone()
        .run_with_resources(
            "interactive document semantic analysis",
            spec,
            None,
            move || {
                let start = Instant::now();
                let result = if current_file_resolution {
                    indexer.process_file_current_file_resolution(&uri, &content, &server)
                } else {
                    indexer.process_file(&uri, &content, &server)
                };
                info!(
                    "[PERF][interactive] file={} mode={} elapsed={:?}",
                    uri.path(),
                    mode_label,
                    start.elapsed()
                );
                result
            },
        )
        .await?
}

/// Initialize workspace and run complete indexing.
///
pub async fn init_workspace(
    server: &RubyLanguageServer,
    folder_uri: Url,
) -> anyhow::Result<crate::indexer::coordinator::IndexingTimings> {
    init_workspace_inner(server, folder_uri, None).await
}

pub async fn init_workspace_for_run(
    server: &RubyLanguageServer,
    folder_uri: Url,
    run: crate::indexing_status::IndexingRun,
) -> anyhow::Result<crate::indexer::coordinator::IndexingTimings> {
    init_workspace_inner(server, folder_uri, Some(run)).await
}

async fn init_workspace_inner(
    server: &RubyLanguageServer,
    folder_uri: Url,
    run: Option<crate::indexing_status::IndexingRun>,
) -> anyhow::Result<crate::indexer::coordinator::IndexingTimings> {
    let workspace_path = folder_uri
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("Failed to convert folder URI to file path"))?;

    info!("Initializing workspace: {:?}", workspace_path);

    let mut coordinator = IndexingCoordinator::new(workspace_path, server.config.lock().clone());
    if let Some(workspace) = server
        .list_workspaces()
        .into_iter()
        .find(|workspace| workspace.root_uri == folder_uri)
    {
        coordinator.set_analysis_engine(workspace.analysis_engine);
    }
    #[cfg(test)]
    if let Some(root) = server.user_cache_root_for_tests() {
        coordinator.set_user_cache_root_for_tests(root);
    }
    coordinator.set_extension_registry(server.extension_registry.clone());
    if let Some(run) = run {
        coordinator.set_indexing_run(run);
    }
    coordinator.run_complete_indexing(server).await?;

    Ok(coordinator.last_timings())
}

pub async fn handle_did_open(server: &RubyLanguageServer, params: DidOpenTextDocumentParams) {
    let total_start = Instant::now();
    let uri = params.text_document.uri.clone();
    let semantic_lock = server.document_semantic_lock(&uri);
    let _semantic_guard = semantic_lock.lock().await;
    let content = params.text_document.text.clone();
    let existing_kind = analysis_file_kind(server, &uri);
    let indexed_content_matches = existing_kind == Some(SourceKind::Project)
        && indexed_disk_content_matches(server, &uri, &content);
    let source_kind = existing_kind.unwrap_or_else(|| source_kind_for_new_open_file(server, &uri));
    let skip_processing = existing_kind
        .map(|kind| kind.is_dependency_source())
        .unwrap_or(false)
        || indexed_content_matches;
    let register_start = Instant::now();
    let analysis_file_id =
        server.open_or_update_analysis_file_with_kind(&uri, content.clone(), source_kind);
    let register_elapsed = register_start.elapsed();

    let doc_start = Instant::now();
    {
        let mut docs = server.docs.lock();
        if let Some(existing_doc) = docs.get(&uri) {
            let mut doc_guard = existing_doc.write();
            doc_guard.set_analysis_file_id(analysis_file_id);
            doc_guard.update(content.clone(), params.text_document.version);
        } else {
            let document = RubyDocument::with_analysis_file_id(
                uri.clone(),
                content.clone(),
                params.text_document.version,
                analysis_file_id,
            );
            docs.insert(uri.clone(), Arc::new(RwLock::new(document)));
        }
    }
    let doc_elapsed = doc_start.elapsed();
    debug!("Doc cache size: {}", server.docs.lock().len());

    // Process file with unified FileProcessor::process_file. Route analysis state
    // by URI so the file lands in its workspace's own index.
    let indexer = interactive_file_processor(server, &uri);

    let process_start = Instant::now();
    let (affected_uris, mut diagnostics) = if skip_processing {
        let diagnostics = if source_kind.is_editable() {
            let document = server
                .docs
                .lock()
                .get(&uri)
                .expect("INVARIANT VIOLATED: didOpen syntax-only path lost the document inserted into the cache. This is a bug because unchanged indexed files still require an open RubyDocument. Fix: keep cache insertion before skip processing.")
                .read()
                .clone();
            let parse_result = document.parse();
            let diagnostics = generate_diagnostics(&parse_result, &document);
            diagnostics
        } else {
            Vec::new()
        };
        if indexed_content_matches {
            server
                .docs
                .lock()
                .get(&uri)
                .expect("INVARIANT VIOLATED: unchanged didOpen document disappeared before indexed-version update. This is a bug because semantic facts were intentionally reused. Fix: keep the open document cached through didOpen.")
                .write()
                .indexed_version = Some(params.text_document.version);
        }
        let mode = if indexed_content_matches {
            "unchanged-index-reuse"
        } else {
            "known-external-skip"
        };
        info!(
            "[PERF][interactive] file={} mode={} elapsed={:?}",
            uri.path(),
            mode,
            process_start.elapsed()
        );
        (std::collections::HashSet::new(), diagnostics)
    } else {
        match process_interactive_file(
            &indexer,
            server,
            &uri,
            &content,
            DocumentSemanticMode::CurrentFile,
        )
        .await
        {
            Ok(result) => (result.affected_uris, result.diagnostics),
            Err(_) => (std::collections::HashSet::new(), Vec::new()),
        }
    };
    if !skip_processing {
        refresh_open_project_files_after_dependency_open(&indexer, server, &uri);
    }
    let process_elapsed = process_start.elapsed();

    let cache_start = Instant::now();
    // Invalidate namespace tree cache with debouncing
    server.invalidate_namespace_tree_cache_debounced();
    debug!("Namespace tree cache invalidation scheduled due to new definitions");
    let cache_elapsed = cache_start.elapsed();

    // Add unresolved entry diagnostics from the analysis engine.
    let diag_start = Instant::now();
    let query = EngineQuery::with_engine(server.analysis_engine_for_uri(&uri));
    diagnostics.extend(query.get_unresolved_diagnostics(&uri));
    append_external_linter_diagnostics(server, &uri, &content, &mut diagnostics).await;
    if !source_kind.is_editable() {
        diagnostics.clear();
    }
    let diag_count = diagnostics.len();
    let diag_elapsed = diag_start.elapsed();
    let publish_start = Instant::now();
    server.publish_diagnostics(uri.clone(), diagnostics).await;
    let publish_elapsed = publish_start.elapsed();

    let affected_start = Instant::now();
    let mut affected_count = 0usize;
    // Publish diagnostics for files affected by removed definitions (cross-file propagation)
    for affected_uri in affected_uris {
        if affected_uri != uri {
            affected_count += 1;
            let affected_diagnostics = query.get_unresolved_diagnostics(&affected_uri);
            server
                .publish_diagnostics(affected_uri, affected_diagnostics)
                .await;
        }
    }
    let affected_elapsed = affected_start.elapsed();
    info!(
        "[PERF][didOpen waterfall] file={} total={:?} register={:?} doc_cache={:?} process={:?} cache_invalidate={:?} diag_query={}@{:?} publish={:?} affected_publish={}@{:?}",
        uri.path(),
        total_start.elapsed(),
        register_elapsed,
        doc_elapsed,
        process_elapsed,
        cache_elapsed,
        diag_count,
        diag_elapsed,
        publish_elapsed,
        affected_count,
        affected_elapsed
    );
}

fn indexed_disk_content_matches(server: &RubyLanguageServer, uri: &Url, content: &str) -> bool {
    let Ok(path) = uri.to_file_path() else {
        return false;
    };
    if !std::fs::read_to_string(&path).is_ok_and(|disk_content| disk_content == content) {
        return false;
    }
    let analysis_engine = server.analysis_engine_for_uri(uri);
    let engine = analysis_engine.read();
    engine
        .file_id(&path)
        .is_some_and(|file_id| engine.file_content_matches(file_id, content))
}

fn refresh_open_project_files_after_dependency_open(
    indexer: &FileProcessor,
    server: &RubyLanguageServer,
    opened_uri: &Url,
) {
    let open_docs = {
        let docs = server.docs.lock();
        docs.iter()
            .filter_map(|(uri, doc)| {
                if uri == opened_uri {
                    return None;
                }
                if analysis_file_kind(server, uri).is_some_and(|kind| kind.is_external()) {
                    return None;
                }
                let doc = doc.read();
                Some((uri.clone(), doc.content.clone()))
            })
            .collect::<Vec<_>>()
    };

    for (uri, content) in open_docs {
        if let Err(err) =
            indexer.process_file_current_file_resolution_forced(&uri, &content, server)
        {
            log::warn!(
                "Failed to refresh open file after dependency open: {}: {err}",
                uri.path()
            );
        }
    }
}

fn source_kind_for_new_open_file(server: &RubyLanguageServer, uri: &Url) -> SourceKind {
    let Some(workspace) = server.workspace_for_uri(uri) else {
        return if server.list_workspaces().is_empty() {
            SourceKind::Project
        } else {
            SourceKind::Excluded
        };
    };
    let Ok(path) = uri.to_file_path() else {
        return SourceKind::Excluded;
    };
    let config = server.config.lock().indexing.clone();
    match ProjectFilePolicy::new(&config) {
        Ok(policy) if policy.includes(&workspace.root_path, &path) => SourceKind::Project,
        Ok(_) => SourceKind::Excluded,
        Err(error) => {
            log::error!("Cannot classify opened file with project source policy: {error}");
            SourceKind::Excluded
        }
    }
}

fn analysis_file_kind(server: &RubyLanguageServer, uri: &Url) -> Option<SourceKind> {
    let path = uri
        .to_file_path()
        .unwrap_or_else(|_| std::path::PathBuf::from(uri.to_string()));
    let analysis_engine = server.analysis_engine_for_uri(uri);
    let engine = analysis_engine.read();
    engine
        .file_id(&path)
        .and_then(|file_id| engine.file(file_id))
        .map(|file| file.kind)
}

pub async fn handle_did_change(server: &RubyLanguageServer, params: DidChangeTextDocumentParams) {
    let total_start = Instant::now();
    let uri = params.text_document.uri.clone();
    let semantic_lock = server.document_semantic_lock(&uri);
    let _semantic_guard = semantic_lock.lock().await;
    let version = params.text_document.version;

    // Get the final content from the last change
    let final_content = match params.content_changes.last() {
        Some(change) => change.text.clone(),
        None => return,
    };
    let register_start = Instant::now();
    let source_kind = analysis_file_kind(server, &uri)
        .unwrap_or_else(|| source_kind_for_new_open_file(server, &uri));
    let analysis_file_id =
        server.open_or_update_analysis_file_with_kind(&uri, final_content.clone(), source_kind);
    let register_elapsed = register_start.elapsed();

    let doc_start = Instant::now();
    // Update or create the document atomically
    {
        let mut docs = server.docs.lock();
        if let Some(existing_doc) = docs.get(&uri) {
            let mut doc_guard = existing_doc.write();
            doc_guard.set_analysis_file_id(analysis_file_id);
            doc_guard.update(final_content.clone(), version);
        } else {
            let new_doc = RubyDocument::with_analysis_file_id(
                uri.clone(),
                final_content.clone(),
                version,
                analysis_file_id,
            );
            docs.insert(uri.clone(), Arc::new(RwLock::new(new_doc)));
        }
    }
    let doc_elapsed = doc_start.elapsed();

    // Process current file without forcing a project-wide reference/diagnostic
    // resolve. Project-wide propagation runs during workspace indexing/save.
    // Route by URI so the file's workspace index is the one updated.
    let indexer = interactive_file_processor(server, &uri);

    let process_start = Instant::now();
    let (affected_uris, mut diagnostics, semantic_change) = match process_interactive_file(
        &indexer,
        server,
        &uri,
        &final_content,
        DocumentSemanticMode::CurrentFile,
    )
    .await
    {
        Ok(result) => (
            result.affected_uris,
            result.diagnostics,
            result.semantic_change,
        ),
        Err(_) => (
            std::collections::HashSet::new(),
            Vec::new(),
            ruby_analysis::engine::SemanticChange::BodyOnly,
        ),
    };
    let process_elapsed = process_start.elapsed();

    // Add unresolved diagnostics (now freshly computed with correct positions)
    let diag_start = Instant::now();
    let query = EngineQuery::with_engine(server.analysis_engine_for_uri(&uri));
    diagnostics.extend(query.get_unresolved_diagnostics(&uri));
    let diag_count = diagnostics.len();
    let diag_elapsed = diag_start.elapsed();

    debug!(
        "Publishing {} diagnostics for {} on change",
        diagnostics.len(),
        uri.path().split('/').next_back().unwrap_or("unknown")
    );
    let publish_start = Instant::now();
    server.publish_diagnostics(uri.clone(), diagnostics).await;
    let publish_elapsed = publish_start.elapsed();

    let open_refresh_start = Instant::now();
    let open_refresh_count =
        if semantic_change == ruby_analysis::engine::SemanticChange::ExportsChanged {
            refresh_bounded_open_diagnostics(server, &indexer, &uri).await
        } else {
            0
        };
    let open_refresh_elapsed = open_refresh_start.elapsed();

    let cache_start = Instant::now();
    // Invalidate namespace tree cache with debouncing
    server.invalidate_namespace_tree_cache_debounced();
    debug!("Namespace tree cache invalidation scheduled due to index change");
    let cache_elapsed = cache_start.elapsed();

    let affected_start = Instant::now();
    let mut affected_count = 0usize;
    // Publish diagnostics for affected files (cross-file propagation)
    for affected_uri in affected_uris {
        if affected_uri != uri {
            affected_count += 1;
            let affected_diagnostics = query.get_unresolved_diagnostics(&affected_uri);
            server
                .publish_diagnostics(affected_uri, affected_diagnostics)
                .await;
        }
    }
    let affected_elapsed = affected_start.elapsed();
    info!(
        "[PERF][didChange waterfall] file={} total={:?} register={:?} doc_cache={:?} process={:?} diag_query={}@{:?} publish={:?} open_refresh={}@{:?} cache_invalidate={:?} affected_publish={}@{:?}",
        uri.path(),
        total_start.elapsed(),
        register_elapsed,
        doc_elapsed,
        process_elapsed,
        diag_count,
        diag_elapsed,
        publish_elapsed,
        open_refresh_count,
        open_refresh_elapsed,
        cache_elapsed,
        affected_count,
        affected_elapsed
    );
}

async fn refresh_bounded_open_diagnostics(
    server: &RubyLanguageServer,
    indexer: &FileProcessor,
    changed_uri: &Url,
) -> usize {
    let open_documents = bounded_open_diagnostic_refresh_targets(server, changed_uri);

    let mut refreshed = 0;
    for (uri, content) in open_documents {
        let Ok(result) =
            indexer.process_file_current_file_resolution_forced(&uri, &content, server)
        else {
            log::warn!("Failed to refresh open-file diagnostics for {}", uri.path());
            continue;
        };
        let query = EngineQuery::with_engine(server.analysis_engine_for_uri(&uri));
        let mut diagnostics = result.diagnostics;
        diagnostics.extend(query.get_unresolved_diagnostics(&uri));
        server.publish_diagnostics(uri, diagnostics).await;
        refreshed += 1;
    }
    refreshed
}

fn bounded_open_diagnostic_refresh_targets(
    server: &RubyLanguageServer,
    changed_uri: &Url,
) -> Vec<(Url, String)> {
    let mut open_documents = {
        let docs = server.docs.lock();
        docs.iter()
            .filter_map(|(uri, document)| {
                if uri == changed_uri {
                    return None;
                }
                Some((uri.clone(), document.read().content.clone()))
            })
            .collect::<Vec<_>>()
    };
    open_documents.retain(|(uri, _)| {
        analysis_file_kind(server, uri).is_some_and(SourceKind::contributes_project_diagnostics)
    });
    open_documents.sort_unstable_by(|left, right| left.0.as_str().cmp(right.0.as_str()));
    open_documents.truncate(MAX_OPEN_DIAGNOSTIC_REFRESH_FILES);
    open_documents
}

pub async fn handle_did_save(server: &RubyLanguageServer, params: DidSaveTextDocumentParams) {
    let uri = params.text_document.uri;
    let semantic_lock = server.document_semantic_lock(&uri);
    let _semantic_guard = semantic_lock.lock().await;
    info!("Document saved: {}", uri.path());

    if !uri.path().ends_with(".rb") {
        return;
    }

    // Get the current document content
    let content = {
        let docs = server.docs.lock();
        match docs.get(&uri) {
            Some(doc_arc) => doc_arc.read().content.clone(),
            None => return,
        }
    };

    // On save: do full indexing with unresolved tracking (for cross-file
    // diagnostics). Route by URI for multi-workspace correctness.
    let indexer = interactive_file_processor(server, &uri);

    let (affected_uris, mut diagnostics) = match process_interactive_file(
        &indexer,
        server,
        &uri,
        &content,
        DocumentSemanticMode::Full,
    )
    .await
    {
        Ok(result) => (result.affected_uris, result.diagnostics),
        Err(_) => (std::collections::HashSet::new(), Vec::new()),
    };

    // Invalidate namespace tree cache
    server.invalidate_namespace_tree_cache_debounced();

    // Add unresolved diagnostics from the analysis engine.
    let query = EngineQuery::with_engine(server.analysis_engine_for_uri(&uri));
    diagnostics.extend(query.get_unresolved_diagnostics(&uri));
    append_external_linter_diagnostics(server, &uri, &content, &mut diagnostics).await;
    server.publish_diagnostics(uri.clone(), diagnostics).await;

    // Publish diagnostics for files affected by removed definitions
    for affected_uri in affected_uris {
        if affected_uri != uri {
            let affected_diagnostics = query.get_unresolved_diagnostics(&affected_uri);
            server
                .publish_diagnostics(affected_uri, affected_diagnostics)
                .await;
        }
    }

    // Request the client to refresh inlay hints after save
    server.refresh_inlay_hints().await;
}

async fn append_external_linter_diagnostics(
    server: &RubyLanguageServer,
    uri: &Url,
    content: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !analysis_file_kind(server, uri).is_some_and(SourceKind::is_editable) {
        return;
    }
    if ruby_analysis::indexer::is_erb_path(uri.path()) {
        return;
    }
    let config = server.config.lock().clone();
    if config.linter == crate::config::LinterKind::None {
        return;
    }
    let Ok(file_path) = uri.to_file_path() else {
        log::warn!(
            "Skipping {} diagnostics for non-file URI {}",
            config.linter.data_name().unwrap_or("external linter"),
            uri
        );
        return;
    };
    let workspace_root = server
        .workspace_for_uri(uri)
        .map(|workspace| workspace.root_path)
        .or_else(|| file_path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| Path::new(".").to_path_buf());

    match lint_document(
        &config,
        server.indexing_resources.clone(),
        &workspace_root,
        &file_path,
        content,
        Duration::from_secs(10),
    )
    .await
    {
        Ok(linter_diagnostics) => diagnostics.extend(linter_diagnostics),
        Err(error) => log::warn!(
            "External linter diagnostics unavailable for {}: {error:#}. \
             Ensure the selected linter is available through the owning project's bundle.",
            file_path.display()
        ),
    }
}

pub async fn handle_did_close(server: &RubyLanguageServer, params: DidCloseTextDocumentParams) {
    let uri = params.text_document.uri.clone();
    let semantic_lock = server.document_semantic_lock(&uri);
    let _semantic_guard = semantic_lock.lock().await;

    // Remove the document from in-memory cache but keep analysis facts.
    server.docs.lock().remove(&uri);
    server.release_external_document_project(&uri);
    debug!("Doc cache size: {}", server.docs.lock().len());

    if clear_file_facts_if_kind(server, &uri, SourceKind::Excluded) {
        server.publish_diagnostics(uri, Vec::new()).await;
        server.invalidate_namespace_tree_cache_debounced();
        return;
    }

    // Keep unresolved entry diagnostics visible (project-wide diagnostics).
    // Use the file's workspace index so we don't surface diagnostics from
    // other workspaces.
    let query = EngineQuery::with_engine(server.analysis_engine_for_uri(&uri));
    let diagnostics = query.get_unresolved_diagnostics(&uri);
    server.publish_diagnostics(uri, diagnostics).await;
}

pub async fn handle_watched_files_changed(
    server: &RubyLanguageServer,
    mut params: DidChangeWatchedFilesParams,
) {
    debug!("Watched files changed: {} files", params.changes.len());
    params
        .changes
        .sort_by(|left, right| left.uri.as_str().cmp(right.uri.as_str()));
    let config = server.config.lock().indexing.clone();
    let policy = match ProjectFilePolicy::new(&config) {
        Ok(policy) => policy,
        Err(error) => {
            log::error!("Cannot apply watched-file source policy: {error}");
            return;
        }
    };
    let processor = FileProcessor::with_extension_registry(server.extension_registry.clone());
    let mut analysis_changed = false;
    let mut changed_dependency_uris = Vec::new();

    for change in params.changes {
        let Some(workspace) = server.workspace_for_uri(&change.uri) else {
            continue;
        };
        let Ok(path) = change.uri.to_file_path() else {
            continue;
        };
        if server.docs.lock().contains_key(&change.uri) {
            continue;
        }
        changed_dependency_uris.push(change.uri.clone());

        if path.extension().is_some_and(|extension| extension == "rbs") {
            if change.typ == FileChangeType::DELETED
                || !policy.includes_signature(&workspace.root_path, &path)
            {
                analysis_changed |=
                    clear_file_facts_if_kind(server, &change.uri, SourceKind::Signature);
                continue;
            }
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => match processor.collect_rbs_facts(&change.uri, &content, server) {
                    Ok(()) => analysis_changed = true,
                    Err(error) => {
                        log::error!(
                            "Failed to index watched RBS file {}: {error}",
                            path.display()
                        );
                        analysis_changed |=
                            clear_file_facts_if_kind(server, &change.uri, SourceKind::Signature);
                    }
                },
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    analysis_changed |=
                        clear_file_facts_if_kind(server, &change.uri, SourceKind::Signature);
                }
                Err(error) => {
                    log::error!(
                        "Failed to read watched RBS file {}: {error}",
                        path.display()
                    );
                    analysis_changed |=
                        clear_file_facts_if_kind(server, &change.uri, SourceKind::Signature);
                }
            }
            continue;
        }

        if change.typ == FileChangeType::DELETED || !policy.includes(&workspace.root_path, &path) {
            analysis_changed |= clear_project_file_facts(server, &change.uri);
            if change.typ == FileChangeType::DELETED {
                server
                    .publish_diagnostics(change.uri.clone(), Vec::new())
                    .await;
            }
            continue;
        }

        match tokio::fs::read_to_string(&path).await {
            Ok(content) => match processor.collect_file_facts(&change.uri, &content, server) {
                Ok(()) => analysis_changed = true,
                Err(error) => {
                    log::error!("Failed to index watched file {}: {error}", path.display());
                    analysis_changed |= clear_project_file_facts(server, &change.uri);
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                analysis_changed |= clear_project_file_facts(server, &change.uri);
            }
            Err(error) => {
                log::error!("Failed to read watched file {}: {error}", path.display());
                analysis_changed |= clear_project_file_facts(server, &change.uri);
            }
        }
    }

    if analysis_changed {
        refresh_open_project_files_for_dependency_engines(
            &processor,
            server,
            &changed_dependency_uris,
        );
        server.invalidate_namespace_tree_cache_debounced();
        debug!("Reindexed watched project files and invalidated namespace tree cache");
    }
}

fn refresh_open_project_files_for_dependency_engines(
    processor: &FileProcessor,
    server: &RubyLanguageServer,
    changed_uris: &[Url],
) {
    let mut changed_engines = Vec::new();
    for uri in changed_uris {
        let engine = server.analysis_engine_for_uri(uri);
        if !changed_engines
            .iter()
            .any(|known| Arc::ptr_eq(known, &engine))
        {
            changed_engines.push(engine);
        }
    }
    if changed_engines.is_empty() {
        return;
    }

    let mut open_project_files = {
        let docs = server.docs.lock();
        docs.iter()
            .filter_map(|(uri, document)| {
                if analysis_file_kind(server, uri) != Some(SourceKind::Project) {
                    return None;
                }
                let owning_engine = server.analysis_engine_for_uri(uri);
                if !changed_engines
                    .iter()
                    .any(|changed| Arc::ptr_eq(changed, &owning_engine))
                {
                    return None;
                }
                Some((uri.clone(), document.read().content.clone()))
            })
            .collect::<Vec<_>>()
    };
    open_project_files.sort_by(|left, right| left.0.as_str().cmp(right.0.as_str()));

    for (uri, content) in open_project_files {
        if let Err(error) =
            processor.process_file_current_file_resolution_forced(&uri, &content, server)
        {
            log::warn!(
                "Failed to refresh open project consumer after dependency change: {}: {error}",
                uri.path()
            );
        }
    }
}

fn clear_project_file_facts(server: &RubyLanguageServer, uri: &Url) -> bool {
    clear_file_facts_if_kind(server, uri, SourceKind::Project)
}

fn clear_file_facts_if_kind(
    server: &RubyLanguageServer,
    uri: &Url,
    expected_kind: SourceKind,
) -> bool {
    let path = uri
        .to_file_path()
        .unwrap_or_else(|_| std::path::PathBuf::from(uri.to_string()));
    let analysis_engine = server.analysis_engine_for_uri(uri);
    let mut engine = analysis_engine.write();
    let Some(file_id) = engine.file_id(&path) else {
        return false;
    };
    if !engine
        .file(file_id)
        .is_some_and(|file| file.kind == expected_kind)
    {
        return false;
    }
    let file_id = engine.register_file(SourceFileInput {
        path,
        content: String::new(),
        kind: expected_kind,
    });
    engine.replace_facts(file_id, FileFacts::default(), ResolveMode::Immediate);
    true
}

#[cfg(test)]
mod tests {
    use ruby_analysis::core::{
        FullyQualifiedName, GraphEdgeKind, MethodFact, NamespaceKind, RubyConstant, RubyMethod,
        SymbolKind, TextRange,
    };
    use ruby_analysis::engine::{AnalysisQuery, FileFacts, ResolveMode};
    use tower_lsp::LanguageServer;

    use super::*;

    fn namespace(name: &str) -> FullyQualifiedName {
        FullyQualifiedName::namespace(vec![
            RubyConstant::new(name).expect("test namespace must be valid")
        ])
    }

    fn has_namespace(server: &RubyLanguageServer, uri: &Url, name: &str) -> bool {
        let analysis_engine = server.analysis_engine_for_uri(uri);
        let engine = analysis_engine.read();
        !AnalysisQuery::new(&engine)
            .symbols_for_fqn(&namespace(name))
            .is_empty()
    }

    #[tokio::test]
    async fn watched_closed_project_files_replace_and_remove_engine_facts() {
        let workspace = tempfile::TempDir::new().unwrap();
        let path = workspace.path().join("watched.rb");
        let uri = Url::from_file_path(&path).unwrap();
        let server = RubyLanguageServer::default();
        server.add_workspace(Url::from_directory_path(workspace.path()).unwrap());

        std::fs::write(&path, "class WatchedOne\nend\n").unwrap();
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: uri.clone(),
                    typ: FileChangeType::CREATED,
                }],
            },
        )
        .await;
        assert!(has_namespace(&server, &uri, "WatchedOne"));

        std::fs::write(&path, "class WatchedTwo\nend\n").unwrap();
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: uri.clone(),
                    typ: FileChangeType::CHANGED,
                }],
            },
        )
        .await;
        assert!(!has_namespace(&server, &uri, "WatchedOne"));
        assert!(has_namespace(&server, &uri, "WatchedTwo"));

        std::fs::remove_file(&path).unwrap();
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: uri.clone(),
                    typ: FileChangeType::DELETED,
                }],
            },
        )
        .await;
        assert!(!has_namespace(&server, &uri, "WatchedTwo"));

        let vendor_path = workspace.path().join("vendor/owned.rb");
        std::fs::create_dir_all(vendor_path.parent().unwrap()).unwrap();
        std::fs::write(&vendor_path, "class VendorOwned\nend\n").unwrap();
        let vendor_uri = Url::from_file_path(&vendor_path).unwrap();
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: vendor_uri.clone(),
                    typ: FileChangeType::CREATED,
                }],
            },
        )
        .await;
        assert!(!has_namespace(&server, &vendor_uri, "VendorOwned"));

        server.config.lock().indexing.included_patterns = vec!["vendor/owned.rb".to_string()];
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: vendor_uri.clone(),
                    typ: FileChangeType::CHANGED,
                }],
            },
        )
        .await;
        assert!(has_namespace(&server, &vendor_uri, "VendorOwned"));
    }

    #[tokio::test]
    async fn watched_project_rbs_files_replace_and_remove_signature_facts() {
        let workspace = tempfile::TempDir::new().unwrap();
        let path = workspace.path().join("sig/native_widget.rbs");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let uri = Url::from_file_path(&path).unwrap();
        let server = RubyLanguageServer::default();
        server.add_workspace(Url::from_directory_path(workspace.path()).unwrap());

        std::fs::write(
            &path,
            "class NativeWidget\n  def encode: () -> String\nend\n",
        )
        .unwrap();
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: uri.clone(),
                    typ: FileChangeType::CREATED,
                }],
            },
        )
        .await;
        assert!(has_namespace(&server, &uri, "NativeWidget"));

        std::fs::write(
            &path,
            "class GeneratedWidget\n  def encode: () -> Integer\nend\n",
        )
        .unwrap();
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: uri.clone(),
                    typ: FileChangeType::CHANGED,
                }],
            },
        )
        .await;
        assert!(!has_namespace(&server, &uri, "NativeWidget"));
        assert!(has_namespace(&server, &uri, "GeneratedWidget"));

        std::fs::write(&path, "class GeneratedWidget\n  def broken: (\n").unwrap();
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: uri.clone(),
                    typ: FileChangeType::CHANGED,
                }],
            },
        )
        .await;
        assert!(
            !has_namespace(&server, &uri, "GeneratedWidget"),
            "malformed regenerated RBS must clear stale signature facts"
        );

        std::fs::remove_file(&path).unwrap();
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: uri.clone(),
                    typ: FileChangeType::DELETED,
                }],
            },
        )
        .await;
        assert!(!has_namespace(&server, &uri, "GeneratedWidget"));
    }

    #[tokio::test]
    async fn watched_rbs_record_refreshes_an_early_open_consumer() {
        let workspace = tempfile::TempDir::new().unwrap();
        let consumer_path = workspace.path().join("consumer.rb");
        let consumer_uri = Url::from_file_path(&consumer_path).unwrap();
        let signature_path = workspace.path().join("sig/payload_factory.rbs");
        std::fs::create_dir_all(signature_path.parent().unwrap()).unwrap();
        let signature_uri = Url::from_file_path(&signature_path).unwrap();
        let server = RubyLanguageServer::default();
        server.add_workspace(Url::from_directory_path(workspace.path()).unwrap());

        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: consumer_uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "payload = PayloadFactory.build\npayload[:name]\n".to_string(),
                },
            },
        )
        .await;
        std::fs::write(
            &signature_path,
            "class PayloadFactory\n  def self.build: () -> { id: Integer, ?name: String }\nend\n",
        )
        .unwrap();
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: signature_uri.clone(),
                    typ: FileChangeType::CREATED,
                }],
            },
        )
        .await;

        let hover = server
            .hover(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: consumer_uri.clone(),
                    },
                    position: Position::new(1, 14),
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
            })
            .await
            .expect("hover request must succeed")
            .expect("the refreshed consumer must publish a keyed-read hover");
        assert!(
            format!("{:?}", hover.contents).contains("String"),
            "the RBS record return must refresh the early-open consumer, got {:?}",
            hover.contents
        );

        std::fs::write(
            &signature_path,
            "class PayloadFactory\n  def self.build: () -> { id: Integer, ?name: Integer }\nend\n",
        )
        .unwrap();
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: signature_uri.clone(),
                    typ: FileChangeType::CHANGED,
                }],
            },
        )
        .await;
        let changed_hover = server
            .hover(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: consumer_uri.clone(),
                    },
                    position: Position::new(1, 14),
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
            })
            .await
            .expect("changed hover request must succeed")
            .expect("the refreshed consumer must retain a keyed-read hover");
        assert!(
            format!("{:?}", changed_hover.contents).contains("Integer"),
            "the replacement RBS record must replace String with Integer, got {:?}",
            changed_hover.contents
        );

        std::fs::remove_file(&signature_path).unwrap();
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: signature_uri,
                    typ: FileChangeType::DELETED,
                }],
            },
        )
        .await;
        let deleted_hover = server
            .hover(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: consumer_uri },
                    position: Position::new(1, 14),
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
            })
            .await
            .expect("post-delete hover request must succeed")
            .expect("the keyed read must retain an explained Unknown hover");
        let deleted = format!("{:?}", deleted_hover.contents);
        assert!(
            deleted.contains("Unknown")
                && !deleted.contains("String")
                && !deleted.contains("Integer"),
            "deleting the RBS contract must remove every stale concrete shape, got {:?}",
            deleted_hover.contents
        );
    }

    #[tokio::test]
    async fn watched_callable_signature_replaces_and_deletes_dependent_results() {
        async fn hover_label(server: &RubyLanguageServer, uri: &Url) -> String {
            let hover = server
                .hover(HoverParams {
                    text_document_position_params: TextDocumentPositionParams {
                        text_document: TextDocumentIdentifier { uri: uri.clone() },
                        position: Position::new(1, 2),
                    },
                    work_done_progress_params: WorkDoneProgressParams::default(),
                })
                .await
                .expect("callable lifecycle hover request must succeed")
                .expect("the callable result local must retain hover evidence");
            format!("{:?}", hover.contents)
        }

        let workspace = tempfile::TempDir::new().unwrap();
        let consumer_path = workspace.path().join("consumer.rb");
        let consumer_uri = Url::from_file_path(&consumer_path).unwrap();
        let signature_path = workspace.path().join("sig/converter.rbs");
        std::fs::create_dir_all(signature_path.parent().unwrap()).unwrap();
        let signature_uri = Url::from_file_path(&signature_path).unwrap();
        let server = RubyLanguageServer::default();
        server.add_workspace(Url::from_directory_path(workspace.path()).unwrap());
        let source = "result = Converter.new.apply(1) { |value| value.to_s }\nresult\n".to_string();

        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: consumer_uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: source,
                },
            },
        )
        .await;

        std::fs::write(
            &signature_path,
            "class Converter\n  def apply: [Input, Output] (Input value) { (Input) -> Output } -> Output\nend\n",
        )
        .unwrap();
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: signature_uri.clone(),
                    typ: FileChangeType::CREATED,
                }],
            },
        )
        .await;
        let created = hover_label(&server, &consumer_uri).await;
        assert!(
            created.contains("String"),
            "created callable signature did not refresh the consumer: {created}"
        );

        std::fs::write(
            &signature_path,
            "class Converter\n  def apply: [Input, Output] (Input value) { (Input) -> Output } -> Array[Output]\nend\n",
        )
        .unwrap();
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: signature_uri.clone(),
                    typ: FileChangeType::CHANGED,
                }],
            },
        )
        .await;
        let changed = hover_label(&server, &consumer_uri).await;
        assert!(
            changed.contains("Array&lt;String&gt;") || changed.contains("Array<String>"),
            "replacement callable signature did not replace the result: {changed}"
        );

        std::fs::remove_file(&signature_path).unwrap();
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: signature_uri,
                    typ: FileChangeType::DELETED,
                }],
            },
        )
        .await;
        let deleted = hover_label(&server, &consumer_uri).await;
        assert!(
            deleted.contains("Unknown")
                && !deleted.contains("String")
                && !deleted.contains("Array"),
            "deleted callable signature left a stale concrete result: {deleted}"
        );
    }

    #[tokio::test]
    async fn opening_default_external_workspace_file_does_not_make_it_project_owned() {
        let workspace = tempfile::TempDir::new().unwrap();
        let path = workspace.path().join("vendor/opened.rb");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let uri = Url::from_file_path(&path).unwrap();
        let server = RubyLanguageServer::default();
        server.add_workspace(Url::from_directory_path(workspace.path()).unwrap());

        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "class OpenedVendor\nend\n".to_string(),
                },
            },
        )
        .await;

        let analysis_engine = server.analysis_engine_for_uri(&uri);
        let engine = analysis_engine.read();
        let file_id = engine
            .file_id(&path)
            .expect("opened workspace file must be registered");
        assert!(
            !engine
                .file(file_id)
                .expect("registered file must exist")
                .kind
                .is_workspace_owned(),
            "default-external workspace files must not become project-owned when opened"
        );
        assert!(
            !AnalysisQuery::new(&engine)
                .symbols_for_fqn(&namespace("OpenedVendor"))
                .is_empty(),
            "opened excluded files must still receive interactive semantic analysis"
        );
        assert!(
            AnalysisQuery::new(&engine)
                .search_workspace_symbols("OpenedVendor", 100)
                .is_empty(),
            "default-external workspace files must stay out of workspace symbols"
        );
        drop(engine);

        handle_did_change(
            &server,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: 2,
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "class ChangedVendor\nend\n".to_string(),
                }],
            },
        )
        .await;

        let analysis_engine = server.analysis_engine_for_uri(&uri);
        let engine = analysis_engine.read();
        assert!(
            !engine
                .file(file_id)
                .expect("changed file must retain its registration")
                .kind
                .is_workspace_owned(),
            "didChange must preserve excluded workspace ownership"
        );
        assert!(
            AnalysisQuery::new(&engine)
                .search_workspace_symbols("ChangedVendor", 100)
                .is_empty(),
            "changed excluded workspace files must stay out of workspace symbols"
        );
        drop(engine);

        handle_did_close(
            &server,
            DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
            },
        )
        .await;

        assert!(
            !has_namespace(&server, &uri, "ChangedVendor"),
            "closing an excluded workspace file must remove its interactive-only facts"
        );
    }

    #[tokio::test]
    async fn did_open_registers_source_in_analysis_engine() {
        let server = RubyLanguageServer::default();
        let uri = Url::parse("file:///tmp/user.rb").expect("test URI must parse");

        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "A = 1".to_string(),
                },
            },
        )
        .await;

        let path = uri.to_file_path().expect("file URI must convert to path");
        let engine = server.analysis_engine.read();
        let file_id = engine
            .file_id(path)
            .expect("did_open must register file in analysis engine");
        let file = engine.file(file_id).unwrap();
        assert_eq!(file.line_index.len(), "A = 1".len());
        assert!(file.source_text().is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn did_open_semantic_pass_waits_for_weighted_admission_without_blocking_reactor() {
        let workspace = tempfile::TempDir::new().unwrap();
        let workspace_root = workspace.path().to_path_buf();
        std::fs::write(
            workspace.path().join("Gemfile"),
            "source 'https://rubygems.org'\n",
        )
        .unwrap();
        let path = workspace.path().join("opened.rb");
        let uri = Url::from_file_path(&path).unwrap();
        let mut server = RubyLanguageServer::default();
        server.indexing_resources = crate::indexing_resources::IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::with_limits(
                1,
                1,
                256 * 1024 * 1024,
                1,
            ),
        );
        server.add_workspace(Url::from_directory_path(workspace.path()).unwrap());

        let release = Arc::new(tokio::sync::Notify::new());
        let holder_release = release.clone();
        let holder_resources = server.indexing_resources.clone();
        let holder_root = workspace_root.clone();
        let holder = tokio::spawn(async move {
            holder_resources
                .run_async_with_resources(
                    "interactive semantic contention holder",
                    crate::indexing_resources::IndexingWorkSpec::new(
                        Some(holder_root),
                        crate::indexing_resources::IndexingResourcePriority::Background,
                        1,
                        256 * 1024 * 1024,
                        1,
                    ),
                    None,
                    async move {
                        holder_release.notified().await;
                    },
                )
                .await
                .unwrap();
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while server.indexing_resources.snapshot().active_tasks != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("resource holder must be admitted before didOpen");

        let open_server = server.clone();
        let open_uri = uri.clone();
        let open = tokio::spawn(async move {
            handle_did_open(
                &open_server,
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri: open_uri,
                        language_id: "ruby".to_string(),
                        version: 1,
                        text: "class OpenedUnderPressure\nend\n".to_string(),
                    },
                },
            )
            .await;
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while server.indexing_resources.snapshot().queued_tasks != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("didOpen semantic work must queue behind the weighted holder");
        tokio::time::timeout(
            Duration::from_millis(50),
            tokio::time::sleep(Duration::from_millis(1)),
        )
        .await
        .expect("queued didOpen must not block the current-thread Tokio reactor");
        assert!(
            !open.is_finished(),
            "didOpen must not bypass weighted admission while resources are saturated"
        );

        release.notify_one();
        holder.await.unwrap();
        open.await.unwrap();
        assert!(has_namespace(&server, &uri, "OpenedUnderPressure"));
        let complete = server.indexing_resources.snapshot();
        assert_eq!(complete.active_tasks, 0);
        assert_eq!(complete.queued_tasks, 0);
        assert_eq!(complete.completed_tasks, 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn overlapping_did_change_versions_cannot_publish_older_semantic_facts() {
        let workspace = tempfile::TempDir::new().unwrap();
        std::fs::write(
            workspace.path().join("Gemfile"),
            "source 'https://rubygems.org'\n",
        )
        .unwrap();
        let path = workspace.path().join("changing.rb");
        let uri = Url::from_file_path(&path).unwrap();
        let mut server = RubyLanguageServer::default();
        server.indexing_resources = crate::indexing_resources::IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::with_limits(
                1,
                1,
                256 * 1024 * 1024,
                1,
            ),
        );
        server.add_workspace(Url::from_directory_path(workspace.path()).unwrap());
        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "class InitialVersion\nend\n".to_string(),
                },
            },
        )
        .await;

        let release = Arc::new(tokio::sync::Notify::new());
        let holder_release = release.clone();
        let holder_resources = server.indexing_resources.clone();
        let holder_root = workspace.path().to_path_buf();
        let holder = tokio::spawn(async move {
            holder_resources
                .run_async_with_resources(
                    "didChange ordering contention holder",
                    crate::indexing_resources::IndexingWorkSpec::new(
                        Some(holder_root),
                        crate::indexing_resources::IndexingResourcePriority::Background,
                        1,
                        256 * 1024 * 1024,
                        1,
                    ),
                    None,
                    async move {
                        holder_release.notified().await;
                    },
                )
                .await
                .unwrap();
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while server.indexing_resources.snapshot().active_tasks != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("contention holder must be admitted");

        let version_two_server = server.clone();
        let version_two_uri = uri.clone();
        let version_two = tokio::spawn(async move {
            handle_did_change(
                &version_two_server,
                DidChangeTextDocumentParams {
                    text_document: VersionedTextDocumentIdentifier {
                        uri: version_two_uri,
                        version: 2,
                    },
                    content_changes: vec![TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text: "class VersionTwo\nend\n".to_string(),
                    }],
                },
            )
            .await;
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while server.indexing_resources.snapshot().queued_tasks != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("version two must queue behind the holder");

        let version_three_server = server.clone();
        let version_three_uri = uri.clone();
        let version_three = tokio::spawn(async move {
            handle_did_change(
                &version_three_server,
                DidChangeTextDocumentParams {
                    text_document: VersionedTextDocumentIdentifier {
                        uri: version_three_uri,
                        version: 3,
                    },
                    content_changes: vec![TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text: "class VersionThree\nend\n".to_string(),
                    }],
                },
            )
            .await;
        });
        tokio::task::yield_now().await;

        release.notify_one();
        holder.await.unwrap();
        version_two.await.unwrap();
        version_three.await.unwrap();
        assert!(
            has_namespace(&server, &uri, "VersionThree"),
            "the newest document version must own the final semantic facts"
        );
        assert!(
            !has_namespace(&server, &uri, "VersionTwo"),
            "an older queued pass must not mark a newer buffer as already indexed"
        );
        assert_eq!(
            server.get_doc(&uri).unwrap().version,
            3,
            "the document cache and semantic facts must agree on the final version"
        );
    }

    #[tokio::test]
    async fn did_open_preserves_known_external_file_without_reprocessing() {
        let server = RubyLanguageServer::default();
        let uri = Url::parse("file:///tmp/rubystubs33/kernel.rb").expect("test URI must parse");
        let file_id = server.open_or_update_analysis_file_with_kind(
            &uri,
            "module Kernel\n  def puts\n  end\nend".to_string(),
            SourceKind::Stub,
        );
        let kernel = RubyConstant::new("Kernel").expect("test constant must be valid");
        let puts = RubyMethod::new("puts").expect("test method must be valid");
        let puts_fqn = FullyQualifiedName::method(vec![kernel], puts);
        server.analysis_engine.write().replace_facts(
            file_id,
            FileFacts {
                methods: vec![MethodFact::new(
                    puts_fqn.clone(),
                    FullyQualifiedName::namespace(vec![kernel]),
                    TextRange::new(file_id, 16, 20),
                )],
                ..Default::default()
            },
            ResolveMode::Deferred,
        );

        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "module Kernel\n  def generated_after_open\n  end\nend".to_string(),
                },
            },
        )
        .await;

        let path = uri.to_file_path().expect("file URI must convert to path");
        let engine = server.analysis_engine.read();
        let file_id = engine
            .file_id(path)
            .expect("known external file must remain registered");
        let file = engine.file(file_id).expect("registered file must exist");
        assert_eq!(file.kind, SourceKind::Stub);
        let query = AnalysisQuery::new(&engine);
        assert_eq!(query.methods_for_fqn(&puts_fqn).len(), 1);
        let generated_fqn = FullyQualifiedName::method(
            vec![kernel],
            RubyMethod::new("generated_after_open").expect("test method must be valid"),
        );
        assert!(
            query.methods_for_fqn(&generated_fqn).is_empty(),
            "known external didOpen must not reprocess and replace indexed stub facts"
        );
    }

    #[tokio::test]
    async fn did_open_reuses_cold_project_facts_when_buffer_matches_indexed_content() {
        let server = RubyLanguageServer::default();
        let workspace_dir = tempfile::tempdir().expect("temporary workspace must be created");
        let workspace_uri = Url::from_directory_path(workspace_dir.path())
            .expect("temporary workspace path must convert to URI");
        let workspace = server.add_workspace(workspace_uri);
        let path = workspace_dir.path().join("user.rb");
        let uri = Url::from_file_path(&path).expect("test path must convert to URI");
        let content = "class User\nend\n";
        std::fs::write(&path, content).expect("cold-indexed test file must be written to disk");
        let file_id = workspace
            .analysis_engine
            .write()
            .register_file(SourceFileInput {
                path,
                content: content.to_string(),
                kind: SourceKind::Project,
            });
        let user = RubyConstant::new("User").expect("test constant must be valid");
        let generated =
            RubyMethod::new("generated_by_extension").expect("test method name must be valid");
        let owner = FullyQualifiedName::namespace(vec![user]);
        let fqn = FullyQualifiedName::method(vec![user], generated);
        workspace.analysis_engine.write().replace_facts(
            file_id,
            FileFacts {
                methods: vec![MethodFact::new(
                    fqn.clone(),
                    owner,
                    TextRange::new(file_id, 0, 5),
                )],
                ..Default::default()
            },
            ResolveMode::Deferred,
        );

        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: content.to_string(),
                },
            },
        )
        .await;

        assert!(
            workspace
                .analysis_engine
                .read()
                .all_method_facts()
                .iter()
                .any(|fact| fact.fqn == fqn),
            "unchanged didOpen must preserve cold-index and extension facts instead of traversing again"
        );
    }

    #[tokio::test]
    async fn did_change_updates_analysis_engine_source() {
        let server = RubyLanguageServer::default();
        let uri = Url::parse("file:///tmp/user.rb").expect("test URI must parse");

        handle_did_change(
            &server,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: 2,
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "A = 2".to_string(),
                }],
            },
        )
        .await;

        let path = uri.to_file_path().expect("file URI must convert to path");
        let engine = server.analysis_engine.read();
        let file_id = engine
            .file_id(path)
            .expect("did_change must register file in analysis engine");
        let file = engine.file(file_id).unwrap();
        assert_eq!(file.line_index.len(), "A = 2".len());
        assert!(file.source_text().is_none());
    }

    #[tokio::test]
    async fn did_change_replaces_analysis_engine_symbol_facts() {
        let server = RubyLanguageServer::default();
        let uri = Url::parse("file:///tmp/user.rb").expect("test URI must parse");

        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "class User\nend".to_string(),
                },
            },
        )
        .await;
        handle_did_change(
            &server,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: 2,
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "class Account\nend".to_string(),
                }],
            },
        )
        .await;

        let user_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
        let account_fqn =
            FullyQualifiedName::namespace(vec![RubyConstant::new("Account").unwrap()]);
        let engine = server.analysis_engine.read();
        assert!(
            engine.symbol_facts_for(&user_fqn).is_empty(),
            "stale User symbol facts must be removed after reindex"
        );
        let account_facts = engine.symbol_facts_for(&account_fqn);
        assert_eq!(account_facts.len(), 1);
        assert_eq!(account_facts[0].kind, SymbolKind::Class);
    }

    #[tokio::test]
    async fn exported_api_change_refreshes_open_consumer_diagnostics() {
        let server = RubyLanguageServer::default();
        let definition_uri = Url::parse("file:///tmp/user.rb").unwrap();
        let consumer_uri = Url::parse("file:///tmp/use_user.rb").unwrap();

        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: definition_uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "class User\n  def name\n    'A'\n  end\nend\n".to_string(),
                },
            },
        )
        .await;
        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: consumer_uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "class User\n  def show\n    name\n  end\nend\n".to_string(),
                },
            },
        )
        .await;
        assert!(
            server
                .last_published_diagnostics(&consumer_uri)
                .iter()
                .all(|diagnostic| diagnostic.code
                    != Some(NumberOrString::String("unresolved-method".to_string()))),
            "existing exported method must keep the open consumer diagnostic-free"
        );

        handle_did_change(
            &server,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: definition_uri,
                    version: 2,
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "class User\nend\n".to_string(),
                }],
            },
        )
        .await;

        assert!(
            server
                .last_published_diagnostics(&consumer_uri)
                .iter()
                .any(|diagnostic| diagnostic.code
                    == Some(NumberOrString::String("unresolved-method".to_string()))),
            "removing an exported method must refresh diagnostics for its open consumer"
        );
    }

    #[tokio::test]
    async fn body_only_change_does_not_refresh_other_open_files() {
        let server = RubyLanguageServer::default();
        let definition_uri = Url::parse("file:///tmp/user.rb").unwrap();
        let consumer_uri = Url::parse("file:///tmp/use_user.rb").unwrap();
        for (uri, text) in [
            (
                definition_uri.clone(),
                "class User\n  def name\n    'A'\n  end\nend\n",
            ),
            (consumer_uri.clone(), "class User\n  name\nend\n"),
        ] {
            handle_did_open(
                &server,
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri,
                        language_id: "ruby".to_string(),
                        version: 1,
                        text: text.to_string(),
                    },
                },
            )
            .await;
        }
        server
            .publish_diagnostics(
                consumer_uri.clone(),
                vec![Diagnostic::new_simple(
                    Range::default(),
                    "sentinel".to_string(),
                )],
            )
            .await;

        handle_did_change(
            &server,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: definition_uri,
                    version: 2,
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "class User\n  def name\n    'B'\n  end\nend\n".to_string(),
                }],
            },
        )
        .await;

        assert_eq!(
            server.last_published_diagnostics(&consumer_uri),
            vec![Diagnostic::new_simple(
                Range::default(),
                "sentinel".to_string(),
            )],
            "body-only typing must not reprocess unrelated open documents"
        );
    }

    #[tokio::test]
    async fn open_diagnostic_refresh_targets_are_sorted_and_capped() {
        let server = RubyLanguageServer::default();
        let changed_uri = Url::parse("file:///tmp/changed.rb").unwrap();
        for index in (0..12).rev() {
            let uri = Url::parse(&format!("file:///tmp/consumer_{index:02}.rb")).unwrap();
            handle_did_open(
                &server,
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri,
                        language_id: "ruby".to_string(),
                        version: 1,
                        text: format!("VALUE_{index} = {index}\n"),
                    },
                },
            )
            .await;
        }

        let targets = bounded_open_diagnostic_refresh_targets(&server, &changed_uri);
        assert_eq!(targets.len(), MAX_OPEN_DIAGNOSTIC_REFRESH_FILES);
        assert_eq!(targets[0].0.path(), "/tmp/consumer_00.rb");
        assert_eq!(targets[7].0.path(), "/tmp/consumer_07.rb");
    }

    #[tokio::test]
    async fn did_open_mirrors_reference_facts_into_analysis_engine() {
        let server = RubyLanguageServer::default();
        let uri = Url::parse("file:///tmp/user.rb").expect("test URI must parse");

        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "class User\nend\nUser.new".to_string(),
                },
            },
        )
        .await;

        let user_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
        let engine = server.analysis_engine.read();
        let query = AnalysisQuery::new(&engine);
        assert_eq!(query.references_for_fqn(&user_fqn).len(), 2);
    }

    #[tokio::test]
    async fn did_open_mirrors_graph_facts_into_analysis_engine() {
        let server = RubyLanguageServer::default();
        let uri = Url::parse("file:///tmp/user.rb").expect("test URI must parse");

        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "module Auth\nend\nclass User\n  include Auth\nend".to_string(),
                },
            },
        )
        .await;

        let user_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
        let auth_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("Auth").unwrap()]);
        let engine = server.analysis_engine.read();
        let query = AnalysisQuery::new(&engine);
        let edges = query.graph_edges_from(&user_fqn);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].target, auth_fqn);
        assert_eq!(edges[0].kind, GraphEdgeKind::Include);
    }

    #[tokio::test]
    async fn did_open_refreshes_late_resolved_graph_facts_into_analysis_engine() {
        let server = RubyLanguageServer::default();
        let user_uri = Url::parse("file:///tmp/user.rb").expect("test URI must parse");
        let auth_uri = Url::parse("file:///tmp/auth.rb").expect("test URI must parse");

        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: user_uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "class User\n  include Auth\nend".to_string(),
                },
            },
        )
        .await;
        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: auth_uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "module Auth\nend".to_string(),
                },
            },
        )
        .await;

        let user_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
        let auth_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("Auth").unwrap()]);
        let engine = server.analysis_engine.read();
        let query = AnalysisQuery::new(&engine);
        let edges = query.graph_edges_from(&user_fqn);
        assert!(
            edges
                .iter()
                .any(|edge| edge.target == auth_fqn && edge.kind == GraphEdgeKind::Include),
            "analysis graph must refresh pending mixin edges once the target module is indexed"
        );
    }

    #[tokio::test]
    async fn did_open_mirrors_normalized_extend_edges_into_analysis_engine() {
        let server = RubyLanguageServer::default();
        let uri = Url::parse("file:///tmp/user.rb").expect("test URI must parse");

        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "module Auth\nend\nclass User\n  extend Auth\nend".to_string(),
                },
            },
        )
        .await;

        let user_singleton =
            FullyQualifiedName::singleton_namespace(vec![RubyConstant::new("User").unwrap()]);
        let auth_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("Auth").unwrap()]);
        let engine = server.analysis_engine.read();
        let query = AnalysisQuery::new(&engine);
        let edges = query.graph_edges_from(&user_singleton);
        assert!(
            edges
                .iter()
                .any(|edge| edge.target == auth_fqn && edge.kind == GraphEdgeKind::Include),
            "extend must be mirrored as a singleton include for analysis method lookup"
        );
    }

    #[tokio::test]
    async fn did_open_mirrors_method_facts_into_analysis_engine() {
        let server = RubyLanguageServer::default();
        let uri = Url::parse("file:///tmp/user.rb").expect("test URI must parse");

        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "class User\n  def name\n  end\n  def self.find\n  end\nend".to_string(),
                },
            },
        )
        .await;

        let user = RubyConstant::new("User").unwrap();
        let name_fqn = FullyQualifiedName::method(
            vec![user],
            RubyMethod::new("name").expect("test method must be valid"),
        );
        let find_fqn = FullyQualifiedName::method(
            vec![user],
            RubyMethod::new("find").expect("test method must be valid"),
        );

        let engine = server.analysis_engine.read();
        let query = AnalysisQuery::new(&engine);
        let name_facts = query.methods_for_fqn(&name_fqn);
        assert_eq!(name_facts.len(), 1);
        assert_eq!(
            name_facts[0].owner.namespace_kind(),
            Some(NamespaceKind::Instance)
        );

        let find_facts = query.methods_for_fqn(&find_fqn);
        assert_eq!(find_facts.len(), 1);
        assert_eq!(
            find_facts[0].owner.namespace_kind(),
            Some(NamespaceKind::Singleton)
        );
    }
}
