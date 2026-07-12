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

fn process_interactive_file(
    indexer: &FileProcessor,
    server: &RubyLanguageServer,
    uri: &Url,
    content: &str,
) -> anyhow::Result<crate::indexer::file_processor::ProcessResult> {
    let start = Instant::now();
    if server.workspace_for_uri(uri).is_some() {
        let result = indexer.process_file_current_file_resolution(uri, content, server);
        info!(
            "[PERF][interactive] file={} mode=current-file elapsed={:?}",
            uri.path(),
            start.elapsed()
        );
        result
    } else {
        let result = indexer.process_file(uri, content, server);
        info!(
            "[PERF][interactive] file={} mode=full-orphan elapsed={:?}",
            uri.path(),
            start.elapsed()
        );
        result
    }
}

/// Initialize workspace and run complete indexing.
///
pub async fn init_workspace(server: &RubyLanguageServer, folder_uri: Url) -> anyhow::Result<()> {
    let workspace_path = folder_uri
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("Failed to convert folder URI to file path"))?;

    info!("Initializing workspace: {:?}", workspace_path);

    let mut coordinator = IndexingCoordinator::new(workspace_path, server.config.lock().clone());
    coordinator.set_extension_registry(server.extension_registry.clone());
    coordinator.run_complete_indexing(server).await?;

    Ok(())
}

pub async fn handle_did_open(server: &RubyLanguageServer, params: DidOpenTextDocumentParams) {
    let total_start = Instant::now();
    let uri = params.text_document.uri.clone();
    let content = params.text_document.text.clone();
    let existing_kind = analysis_file_kind(server, &uri);
    let source_kind = existing_kind.unwrap_or_else(|| source_kind_for_new_open_file(server, &uri));
    let skip_processing = existing_kind
        .map(|kind| kind.is_dependency_source())
        .unwrap_or(false);
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
    let indexer = FileProcessor::with_extension_registry(server.extension_registry.clone());

    let process_start = Instant::now();
    let (affected_uris, mut diagnostics) = if skip_processing {
        info!(
            "[PERF][interactive] file={} mode=known-external-skip elapsed={:?}",
            uri.path(),
            process_start.elapsed()
        );
        (std::collections::HashSet::new(), Vec::new())
    } else {
        match process_interactive_file(&indexer, server, &uri, &content) {
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
    let query = EngineQuery::with_engine(server.analysis_engine.clone());
    diagnostics.extend(query.get_unresolved_diagnostics(&uri));
    append_external_linter_diagnostics(server, &uri, &content, &mut diagnostics).await;
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

fn source_kind_for_new_open_file(server: &RubyLanguageServer, uri: &Url) -> SourceKind {
    let Some(workspace) = server.workspace_for_uri(uri) else {
        return SourceKind::Project;
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

fn analysis_file_kind(server: &RubyLanguageServer, uri: &Url) -> Option<SourceKind> {
    let path = uri
        .to_file_path()
        .unwrap_or_else(|_| std::path::PathBuf::from(uri.to_string()));
    let engine = server.analysis_engine.read();
    engine
        .file_id(&path)
        .and_then(|file_id| engine.file(file_id))
        .map(|file| file.kind)
}

pub async fn handle_did_change(server: &RubyLanguageServer, params: DidChangeTextDocumentParams) {
    let total_start = Instant::now();
    let uri = params.text_document.uri.clone();
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
    let indexer = FileProcessor::with_extension_registry(server.extension_registry.clone());

    let process_start = Instant::now();
    let (affected_uris, mut diagnostics) =
        match process_interactive_file(&indexer, server, &uri, &final_content) {
            Ok(result) => (result.affected_uris, result.diagnostics),
            Err(_) => (std::collections::HashSet::new(), Vec::new()),
        };
    let process_elapsed = process_start.elapsed();

    // Add unresolved diagnostics (now freshly computed with correct positions)
    let diag_start = Instant::now();
    let query = EngineQuery::with_engine(server.analysis_engine.clone());
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
        "[PERF][didChange waterfall] file={} total={:?} register={:?} doc_cache={:?} process={:?} diag_query={}@{:?} publish={:?} cache_invalidate={:?} affected_publish={}@{:?}",
        uri.path(),
        total_start.elapsed(),
        register_elapsed,
        doc_elapsed,
        process_elapsed,
        diag_count,
        diag_elapsed,
        publish_elapsed,
        cache_elapsed,
        affected_count,
        affected_elapsed
    );
}

pub async fn handle_did_save(server: &RubyLanguageServer, params: DidSaveTextDocumentParams) {
    let uri = params.text_document.uri;
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
    let indexer = FileProcessor::with_extension_registry(server.extension_registry.clone());

    let (affected_uris, mut diagnostics) = match indexer.process_file(&uri, &content, server) {
        Ok(result) => (result.affected_uris, result.diagnostics),
        Err(_) => (std::collections::HashSet::new(), Vec::new()),
    };

    // Invalidate namespace tree cache
    server.invalidate_namespace_tree_cache_debounced();

    // Add unresolved diagnostics from the analysis engine.
    let query = EngineQuery::with_engine(server.analysis_engine.clone());
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
             Check rubyFastLsp.linterCommand and the workspace bundle.",
            file_path.display()
        ),
    }
}

pub async fn handle_did_close(server: &RubyLanguageServer, params: DidCloseTextDocumentParams) {
    let uri = params.text_document.uri.clone();

    // Remove the document from in-memory cache but keep analysis facts.
    server.docs.lock().remove(&uri);
    debug!("Doc cache size: {}", server.docs.lock().len());

    if clear_file_facts_if_kind(server, &uri, SourceKind::Excluded) {
        server.publish_diagnostics(uri, Vec::new()).await;
        server.invalidate_namespace_tree_cache_debounced();
        return;
    }

    // Keep unresolved entry diagnostics visible (project-wide diagnostics).
    // Use the file's workspace index so we don't surface diagnostics from
    // other workspaces.
    let query = EngineQuery::with_engine(server.analysis_engine.clone());
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
        server.invalidate_namespace_tree_cache_debounced();
        debug!("Reindexed watched project files and invalidated namespace tree cache");
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
    let mut engine = server.analysis_engine.write();
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

    use super::*;

    fn namespace(name: &str) -> FullyQualifiedName {
        FullyQualifiedName::namespace(vec![
            RubyConstant::new(name).expect("test namespace must be valid")
        ])
    }

    fn has_namespace(server: &RubyLanguageServer, name: &str) -> bool {
        let engine = server.analysis_engine.read();
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
        assert!(has_namespace(&server, "WatchedOne"));

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
        assert!(!has_namespace(&server, "WatchedOne"));
        assert!(has_namespace(&server, "WatchedTwo"));

        std::fs::remove_file(&path).unwrap();
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri,
                    typ: FileChangeType::DELETED,
                }],
            },
        )
        .await;
        assert!(!has_namespace(&server, "WatchedTwo"));

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
        assert!(!has_namespace(&server, "VendorOwned"));

        server.config.lock().indexing.included_patterns = vec!["vendor/owned.rb".to_string()];
        handle_watched_files_changed(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: vendor_uri,
                    typ: FileChangeType::CHANGED,
                }],
            },
        )
        .await;
        assert!(has_namespace(&server, "VendorOwned"));
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

        let engine = server.analysis_engine.read();
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

        let engine = server.analysis_engine.read();
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
                text_document: TextDocumentIdentifier { uri },
            },
        )
        .await;

        assert!(
            !has_namespace(&server, "ChangedVendor"),
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
