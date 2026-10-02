use crate::features::cursor::EngineQuery;
use crate::features::diagnostics::linter::lint_document;
use crate::invariant::ExpectInvariant;
use crate::loader::coordinator::IndexingCoordinator;
use crate::loader::file_processor::syntax_diagnostics::generate_diagnostics;
use crate::loader::file_processor::FileProcessor;
use crate::loader::sources::project::files::ProjectFilePolicy;
use crate::server::RubyLanguageServer;
use ruby_analysis::core::FileAnalysis;
use ruby_analysis::core::SourceKind;
use ruby_analysis::engine::{ResolveMode, SourceFileInput};

use log::{debug, info};
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
    let processor = FileProcessor::with_extension_registry(server.extensions.registry().clone());
    server
        .jruby_add_on_for_uri(uri)
        .map(|add_on| processor.clone().with_jruby_add_on(&add_on))
        .unwrap_or(processor)
}

async fn process_interactive_file(
    indexer: &FileProcessor,
    server: &RubyLanguageServer,
    uri: &Url,
    content: &str,
    mode: DocumentSemanticMode,
) -> anyhow::Result<crate::loader::file_processor::ProcessResult> {
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
    let spec = crate::utils::admission::IndexingWorkSpec::new(
        project_root,
        crate::utils::admission::IndexingResourcePriority::OpenDocument,
        1,
        INTERACTIVE_SEMANTIC_TRANSIENT_MEMORY_BYTES,
        1,
    );
    server
        .indexing
        .resources()
        .clone()
        .run_with_resources(
            "interactive document semantic analysis",
            spec,
            None,
            move || {
                let start = Instant::now();
                let ctx = server.load_context_for_uri(&uri);
                let loaded = if current_file_resolution {
                    indexer.analyze_file_current_file_resolution(&uri, &content, &ctx)
                } else {
                    indexer.analyze_file(&uri, &content, &ctx)
                };
                let result = loaded.map(|loaded| loaded.commit(&ctx));
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

/// Reanalyze an open document after another file changed what it depends on,
/// and commit it immediately with current-file resolution. Runs even when the
/// document's version is already indexed.
fn reanalyze_open_file(
    processor: &FileProcessor,
    server: &RubyLanguageServer,
    uri: &Url,
    content: &str,
) -> anyhow::Result<crate::loader::file_processor::ProcessResult> {
    let ctx = server.load_context_for_uri(uri);
    let loaded = processor.analyze_file_current_file_resolution_forced(uri, content, &ctx)?;
    Ok(loaded.commit(&ctx))
}

/// Initialize workspace and run complete indexing.
///
pub async fn init_workspace(
    server: &RubyLanguageServer,
    folder_uri: Url,
) -> anyhow::Result<crate::loader::coordinator::IndexingTimings> {
    init_workspace_inner(server, folder_uri, None).await
}

pub async fn init_workspace_for_run(
    server: &RubyLanguageServer,
    folder_uri: Url,
    run: crate::loader::scheduling::status::IndexingRun,
) -> anyhow::Result<crate::loader::coordinator::IndexingTimings> {
    init_workspace_inner(server, folder_uri, Some(run)).await
}

async fn init_workspace_inner(
    server: &RubyLanguageServer,
    folder_uri: Url,
    run: Option<crate::loader::scheduling::status::IndexingRun>,
) -> anyhow::Result<crate::loader::coordinator::IndexingTimings> {
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
    coordinator.set_cache_root(server.products.cache_root());
    coordinator.set_extension_registry(server.extensions.registry().clone());
    if let Some(run) = run {
        coordinator.set_indexing_run(run);
    }
    let ctx = server.load_context_for_project(coordinator.workspace_root());
    coordinator.run_complete_indexing(&ctx).await?;

    Ok(coordinator.last_timings())
}

pub async fn handle_did_open(server: &RubyLanguageServer, params: DidOpenTextDocumentParams) {
    let total_start = Instant::now();
    let uri = params.text_document.uri.clone();
    let semantic_lock = server.document_semantic_lock(&uri);
    let _semantic_guard = semantic_lock.lock().await;
    let content = params.text_document.text.clone();
    let existing_kind = analysis_file_kind(server, &uri);
    let source_kind = existing_kind.unwrap_or_else(|| source_kind_for_new_open_file(server, &uri));
    // Facts from cold indexing do not carry the open document's local
    // variable scopes, so every project file is analyzed again when opened.
    let skip_processing = existing_kind.is_some_and(|kind| kind.is_dependency_source());
    let register_start = Instant::now();
    let analysis_file_id =
        server.open_or_update_analysis_file_with_kind(&uri, content.clone(), source_kind);
    let register_elapsed = register_start.elapsed();
    #[cfg(test)]
    if let Ok(path) = uri.to_file_path() {
        server
            .indexing
            .schedule
            .checkpoint(
                crate::loader::scheduling::test_schedule::Point::DocumentSourceUpdated,
                &path,
            )
            .await;
    }

    let doc_start = Instant::now();
    server.documents.update(
        &uri,
        analysis_file_id,
        content.clone(),
        params.text_document.version,
    );
    let doc_elapsed = doc_start.elapsed();
    debug!("Doc cache size: {}", server.documents.read().len());

    // Analyze and commit the file with the unified FileProcessor. Route analysis state
    // by URI so the file lands in its workspace's own index.
    let indexer = interactive_file_processor(server, &uri);

    let process_start = Instant::now();
    let (affected_uris, mut diagnostics) = if skip_processing {
        let diagnostics = if source_kind.is_editable() {
            let document = server
                .documents
                .read()
                .get(&uri)
                .expect_invariant(
                    "didOpen syntax-only path lost the document inserted into the cache",
                    "skipped dependency files still require an open RubyDocument",
                    "keep cache insertion before skip processing",
                )
                .read()
                .clone();
            let parse_result = document.parse();
            let diagnostics = generate_diagnostics(&parse_result, &document);
            diagnostics
        } else {
            Vec::new()
        };
        info!(
            "[PERF][interactive] file={} mode=known-external-skip elapsed={:?}",
            uri.path(),
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
        refresh_open_project_files_after_dependency_open(&indexer, server, &uri).await;
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

async fn refresh_open_project_files_after_dependency_open(
    indexer: &FileProcessor,
    server: &RubyLanguageServer,
    opened_uri: &Url,
) {
    let owning_engine = server.analysis_engine_for_uri(opened_uri);
    let mut open_docs = {
        let docs = server.documents.read();
        docs.iter()
            .filter_map(|(uri, doc)| {
                if uri == opened_uri {
                    return None;
                }
                if !Arc::ptr_eq(&owning_engine, &server.analysis_engine_for_uri(uri)) {
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
    open_docs.sort_unstable_by(|left, right| left.0.as_str().cmp(right.0.as_str()));

    for (uri, content) in open_docs {
        match reanalyze_open_file(indexer, server, &uri, &content) {
            Ok(result) => {
                let mut diagnostics = result.diagnostics;
                diagnostics.extend(
                    EngineQuery::with_engine(owning_engine.clone())
                        .get_unresolved_diagnostics(&uri),
                );
                server.append_current_external_linter_diagnostics(&uri, &mut diagnostics);
                server.publish_diagnostics(uri, diagnostics).await;
            }
            Err(err) => log::warn!(
                "Failed to refresh open file after dependency open: {}: {err}",
                uri.path()
            ),
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
        .view()
        .file_id(&path)
        .and_then(|file_id| engine.view().file(file_id))
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
    server.clear_external_linter_diagnostics(&uri);
    let register_start = Instant::now();
    let source_kind = analysis_file_kind(server, &uri)
        .unwrap_or_else(|| source_kind_for_new_open_file(server, &uri));
    let analysis_file_id =
        server.open_or_update_analysis_file_with_kind(&uri, final_content.clone(), source_kind);
    let register_elapsed = register_start.elapsed();
    #[cfg(test)]
    if let Ok(path) = uri.to_file_path() {
        server
            .indexing
            .schedule
            .checkpoint(
                crate::loader::scheduling::test_schedule::Point::DocumentSourceUpdated,
                &path,
            )
            .await;
    }

    let doc_start = Instant::now();
    // Update or create the document atomically
    server
        .documents
        .update(&uri, analysis_file_id, final_content.clone(), version);
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
        let Ok(result) = reanalyze_open_file(indexer, server, &uri, &content) else {
            log::warn!("Failed to refresh open-file diagnostics for {}", uri.path());
            continue;
        };
        let query = EngineQuery::with_engine(server.analysis_engine_for_uri(&uri));
        let mut diagnostics = result.diagnostics;
        diagnostics.extend(query.get_unresolved_diagnostics(&uri));
        server.append_current_external_linter_diagnostics(&uri, &mut diagnostics);
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
        let docs = server.documents.read();
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
        let docs = server.documents.read();
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
    server.clear_external_linter_diagnostics(uri);
    if !analysis_file_kind(server, uri).is_some_and(SourceKind::is_editable) {
        return;
    }
    if ruby_analysis::indexer::is_erb_path(uri.path()) {
        return;
    }
    let config = server.config.lock().clone();
    if config.linter == crate::environment::config::LinterKind::None {
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
    let Some(snapshot) = server.diagnostic_source_snapshot(uri) else {
        return;
    };

    match lint_document(
        &config,
        server.indexing.resources().clone(),
        &workspace_root,
        &file_path,
        content,
        Duration::from_secs(10),
    )
    .await
    {
        Ok(linter_diagnostics) => {
            if server.retain_external_linter_diagnostics(uri, snapshot, &linter_diagnostics) {
                diagnostics.extend(linter_diagnostics);
            }
        }
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
    server.clear_external_linter_diagnostics(&uri);

    // Remove the document from in-memory cache but keep analysis facts.
    server.documents.remove(&uri);
    server.release_external_document_project(&uri);
    debug!("Doc cache size: {}", server.documents.read().len());

    // An excluded file is analyzed only while it is open.
    if remove_file_if_kind(server, &uri, SourceKind::Excluded) {
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
    let processor = FileProcessor::with_extension_registry(server.extensions.registry().clone());
    let mut analysis_changed = false;
    let mut changed_dependency_uris = Vec::new();

    for change in params.changes {
        let Some(workspace) = server.workspace_for_uri(&change.uri) else {
            continue;
        };
        let Ok(path) = change.uri.to_file_path() else {
            continue;
        };
        if server.documents.read().contains_key(&change.uri) {
            continue;
        }
        changed_dependency_uris.push(change.uri.clone());

        if path.extension().is_some_and(|extension| extension == "rbs") {
            if change.typ == FileChangeType::DELETED
                || !policy.includes_signature(&workspace.root_path, &path)
            {
                analysis_changed |= remove_file_if_kind(server, &change.uri, SourceKind::Signature);
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
                        remove_file_if_kind(server, &change.uri, SourceKind::Signature);
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
            analysis_changed |= remove_project_file(server, &change.uri);
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
                analysis_changed |= remove_project_file(server, &change.uri);
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
        )
        .await;
        server.invalidate_namespace_tree_cache_debounced();
        debug!("Reindexed watched project files and invalidated namespace tree cache");
    }
}

async fn refresh_open_project_files_for_dependency_engines(
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
        let docs = server.documents.read();
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
        match reanalyze_open_file(processor, server, &uri, &content) {
            Ok(result) => {
                let query = EngineQuery::with_engine(server.analysis_engine_for_uri(&uri));
                let mut diagnostics = result.diagnostics;
                diagnostics.extend(query.get_unresolved_diagnostics(&uri));
                server.append_current_external_linter_diagnostics(&uri, &mut diagnostics);
                server.publish_diagnostics(uri, diagnostics).await;
            }
            Err(error) => log::warn!(
                "Failed to refresh open project consumer after dependency change: {}: {error}",
                uri.path()
            ),
        }
    }
}

fn remove_project_file(server: &RubyLanguageServer, uri: &Url) -> bool {
    remove_file_if_kind(server, uri, SourceKind::Project)
}

fn clear_project_file_facts(server: &RubyLanguageServer, uri: &Url) -> bool {
    clear_file_facts_if_kind(server, uri, SourceKind::Project)
}

/// Remove a file that no longer exists or no longer belongs to its project.
/// Other files stop resolving into it, and a later registration at the same
/// path starts from a fresh identity.
fn remove_file_if_kind(server: &RubyLanguageServer, uri: &Url, expected_kind: SourceKind) -> bool {
    let analysis_engine = server.analysis_engine_for_uri(uri);
    let mut engine = analysis_engine.write();
    let Some(file_id) = registered_file_of_kind(&engine, uri, expected_kind) else {
        return false;
    };
    engine.remove(file_id, ResolveMode::Immediate)
}

/// Keep a file that still exists on disk but could not be read or analyzed
/// registered as an empty source, so require resolution still finds it while
/// none of its previous facts survive.
fn clear_file_facts_if_kind(
    server: &RubyLanguageServer,
    uri: &Url,
    expected_kind: SourceKind,
) -> bool {
    let analysis_engine = server.analysis_engine_for_uri(uri);
    let mut engine = analysis_engine.write();
    let Some(file_id) = registered_file_of_kind(&engine, uri, expected_kind) else {
        return false;
    };
    let path = engine
        .view()
        .file(file_id)
        .map(|file| file.path.clone())
        .expect_invariant(
            "a registered file vanished under the engine write lock",
            "the lookup and the clear hold one engine write borrow",
            "keep lookup and clear inside one write guard",
        );
    let file_id = engine.register_file(SourceFileInput {
        path,
        content: String::new(),
        kind: expected_kind,
    });
    engine.update(file_id, FileAnalysis::default(), ResolveMode::Immediate);
    true
}

fn registered_file_of_kind(
    engine: &ruby_analysis::engine::Project,
    uri: &Url,
    expected_kind: SourceKind,
) -> Option<ruby_analysis::core::SourceFileId> {
    let path = uri
        .to_file_path()
        .unwrap_or_else(|_| std::path::PathBuf::from(uri.to_string()));
    let file_id = engine.view().file_id(&path)?;
    engine
        .view()
        .file(file_id)
        .is_some_and(|file| file.kind == expected_kind)
        .then_some(file_id)
}

#[cfg(test)]
mod tests;
