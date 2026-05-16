use crate::indexer::coordinator::IndexingCoordinator;
use crate::indexer::file_processor::FileProcessor;
use crate::indexer::file_processor::ProcessingOptions;
use crate::query::IndexQuery;
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;

use log::{debug, info};
use parking_lot::RwLock;
use std::sync::Arc;
use tower_lsp::lsp_types::*;

/// Initialize workspace and run complete indexing.
///
/// Routes the workspace's index by URI: the matching `Workspace` (registered
/// via `server.add_workspace`) supplies the `Index<Unlocked>` that the
/// coordinator writes into. Files outside any registered workspace fall back
/// to `server.orphan_index`.
pub async fn init_workspace(server: &RubyLanguageServer, folder_uri: Url) -> anyhow::Result<()> {
    let workspace_path = folder_uri
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("Failed to convert folder URI to file path"))?;

    info!("Initializing workspace: {:?}", workspace_path);

    let index = server.index_for_uri(&folder_uri);
    let mut coordinator =
        IndexingCoordinator::new(workspace_path, server.config.lock().clone(), index);
    coordinator.set_extension_registry(server.extension_registry.clone());
    coordinator.run_complete_indexing(server).await?;

    Ok(())
}

pub async fn handle_did_open(server: &RubyLanguageServer, params: DidOpenTextDocumentParams) {
    let uri = params.text_document.uri.clone();
    let content = params.text_document.text.clone();
    let analysis_file_id = server.open_or_update_analysis_file(&uri, content.clone());

    // Only create a fresh document if one doesn't exist
    // IMPORTANT: Don't overwrite existing document that may have lvars from workspace indexing
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
    debug!("Doc cache size: {}", server.docs.lock().len());

    // Process file with unified FileProcessor::process_file. Route the index
    // by URI so the file lands in its workspace's own index.
    let workspace_index = server.index_for_uri(&uri);
    let indexer = FileProcessor::with_extension_registry(
        workspace_index.clone(),
        server.extension_registry.clone(),
    );
    let options = ProcessingOptions {
        index_definitions: true,
        index_references: true,
        resolve_mixins: true, // resolve mixins on open
        include_local_vars: true,
    };

    let (affected_uris, mut diagnostics) =
        match indexer.process_file(&uri, &content, server, options) {
            Ok(result) => (result.affected_uris, result.diagnostics),
            Err(_) => (std::collections::HashSet::new(), Vec::new()),
        };

    // Invalidate namespace tree cache with debouncing
    server.invalidate_namespace_tree_cache_debounced();
    debug!("Namespace tree cache invalidation scheduled due to new definitions");

    // Add unresolved entry diagnostics and YARD diagnostics (no re-parsing needed)
    let query = IndexQuery::with_engine(workspace_index, server.analysis_engine.clone());
    diagnostics.extend(query.get_unresolved_diagnostics(&uri));
    diagnostics.extend(query.get_yard_diagnostics(&uri));
    server.publish_diagnostics(uri.clone(), diagnostics).await;

    // Publish diagnostics for files affected by removed definitions (cross-file propagation)
    for affected_uri in affected_uris {
        if affected_uri != uri {
            let affected_diagnostics = query.get_unresolved_diagnostics(&affected_uri);
            server
                .publish_diagnostics(affected_uri, affected_diagnostics)
                .await;
        }
    }
}

pub async fn handle_did_change(server: &RubyLanguageServer, params: DidChangeTextDocumentParams) {
    let uri = params.text_document.uri.clone();
    let version = params.text_document.version;

    // Get the final content from the last change
    let final_content = match params.content_changes.last() {
        Some(change) => change.text.clone(),
        None => return,
    };
    let analysis_file_id = server.open_or_update_analysis_file(&uri, final_content.clone());

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

    // Full processing on every change - includes unresolved diagnostics.
    // Route by URI so the file's workspace index is the one updated.
    let workspace_index = server.index_for_uri(&uri);
    let indexer = FileProcessor::with_extension_registry(
        workspace_index.clone(),
        server.extension_registry.clone(),
    );
    let options = ProcessingOptions {
        index_definitions: true,
        index_references: true,
        resolve_mixins: true, // Must resolve mixins to keep inheritance graph up-to-date
        include_local_vars: true,
    };

    let (affected_uris, mut diagnostics) =
        match indexer.process_file(&uri, &final_content, server, options) {
            Ok(result) => (result.affected_uris, result.diagnostics),
            Err(_) => (std::collections::HashSet::new(), Vec::new()),
        };

    // Add unresolved diagnostics (now freshly computed with correct positions)
    let query = IndexQuery::with_engine(workspace_index, server.analysis_engine.clone());
    diagnostics.extend(query.get_unresolved_diagnostics(&uri));

    debug!(
        "Publishing {} diagnostics for {} on change",
        diagnostics.len(),
        uri.path().split('/').next_back().unwrap_or("unknown")
    );
    server.publish_diagnostics(uri.clone(), diagnostics).await;

    // Invalidate namespace tree cache with debouncing
    server.invalidate_namespace_tree_cache_debounced();
    debug!("Namespace tree cache invalidation scheduled due to index change");

    // Publish diagnostics for affected files (cross-file propagation)
    for affected_uri in affected_uris {
        if affected_uri != uri {
            let affected_diagnostics = query.get_unresolved_diagnostics(&affected_uri);
            server
                .publish_diagnostics(affected_uri, affected_diagnostics)
                .await;
        }
    }
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
    let workspace_index = server.index_for_uri(&uri);
    let indexer = FileProcessor::with_extension_registry(
        workspace_index.clone(),
        server.extension_registry.clone(),
    );
    let options = ProcessingOptions {
        index_definitions: true,
        index_references: true,
        resolve_mixins: true, // resolve mixins on save
        include_local_vars: true,
    };

    let (affected_uris, mut diagnostics) =
        match indexer.process_file(&uri, &content, server, options) {
            Ok(result) => (result.affected_uris, result.diagnostics),
            Err(_) => (std::collections::HashSet::new(), Vec::new()),
        };

    // Invalidate namespace tree cache
    server.invalidate_namespace_tree_cache_debounced();

    // Add unresolved diagnostics and YARD diagnostics (no re-parsing needed)
    let query = IndexQuery::with_engine(workspace_index, server.analysis_engine.clone());
    diagnostics.extend(query.get_unresolved_diagnostics(&uri));
    diagnostics.extend(query.get_yard_diagnostics(&uri));
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

pub async fn handle_did_close(server: &RubyLanguageServer, params: DidCloseTextDocumentParams) {
    let uri = params.text_document.uri.clone();

    // Remove the document from in-memory cache but keep definitions/references in the index
    server.docs.lock().remove(&uri);
    debug!("Doc cache size: {}", server.docs.lock().len());

    // Keep unresolved entry diagnostics visible (project-wide diagnostics).
    // Use the file's workspace index so we don't surface diagnostics from
    // other workspaces.
    let query = IndexQuery::with_engine(server.index_for_uri(&uri), server.analysis_engine.clone());
    let diagnostics = query.get_unresolved_diagnostics(&uri);
    server.publish_diagnostics(uri, diagnostics).await;
}

pub async fn handle_watched_files_changed(
    server: &RubyLanguageServer,
    params: DidChangeWatchedFilesParams,
) {
    debug!("Watched files changed: {} files", params.changes.len());

    let has_ruby_changes = params
        .changes
        .iter()
        .any(|change| change.uri.path().ends_with(".rb"));

    if has_ruby_changes {
        server.invalidate_namespace_tree_cache_debounced();
        debug!("Scheduled namespace tree cache invalidation for watched file changes");
    }
}

#[cfg(test)]
mod tests {
    use ruby_analysis_core::{
        FullyQualifiedName, GraphEdgeKind, NamespaceKind, RubyConstant, RubyMethod, SymbolKind,
    };
    use ruby_analysis_engine::AnalysisQuery;

    use super::*;

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
        let engine = server.analysis_engine.lock();
        let file_id = engine
            .file_id(path)
            .expect("did_open must register file in analysis engine");
        assert_eq!(engine.file(file_id).unwrap().source, "A = 1");
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
        let engine = server.analysis_engine.lock();
        let file_id = engine
            .file_id(path)
            .expect("did_change must register file in analysis engine");
        assert_eq!(engine.file(file_id).unwrap().source, "A = 2");
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
        let engine = server.analysis_engine.lock();
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
        let engine = server.analysis_engine.lock();
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
        let engine = server.analysis_engine.lock();
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
        let engine = server.analysis_engine.lock();
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
        let engine = server.analysis_engine.lock();
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
            vec![user.clone()],
            RubyMethod::new("name").expect("test method must be valid"),
        );
        let find_fqn = FullyQualifiedName::method(
            vec![user.clone()],
            RubyMethod::new("find").expect("test method must be valid"),
        );

        let engine = server.analysis_engine.lock();
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
