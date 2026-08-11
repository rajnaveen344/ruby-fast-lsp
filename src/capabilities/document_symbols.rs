use log::{debug, info, warn};
use ruby_prism::Visit;
use std::time::Instant;
use tower_lsp::lsp_types::{DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse};

use crate::server::RubyLanguageServer;
use crate::utils::lsp::lsp_range;

use ruby_analysis::core::NamespaceKind;
use ruby_analysis::indexer::{
    DocumentSymbolKind, DocumentSymbolsVisitor, MethodVisibility, RubySymbolContext,
};

/// Handle document symbols request for a Ruby file
pub async fn handle_document_symbols(
    server: &RubyLanguageServer,
    params: DocumentSymbolParams,
) -> Option<DocumentSymbolResponse> {
    let uri = params.text_document.uri;

    info!("Document symbols request for: {}", uri.path());
    let start_time = Instant::now();

    // Get document content from server cache
    let document = match server.get_doc(&uri) {
        Some(doc) => doc,
        None => {
            info!("Document not found in cache for URI: {}", uri);
            return None;
        }
    };

    let mut lsp_symbols = {
        // Prism parse results and AST nodes are not Send. Keep them in this
        // scope so they are destroyed before the governed extension await.
        let parse_result = document.parse();
        let parse_time = start_time.elapsed();
        debug!("[PERF] Document symbols parse took {:?}", parse_time);

        let root_node = parse_result.node();
        let mut visitor = DocumentSymbolsVisitor::new(&document);
        visitor.visit(&root_node);
        let ruby_symbols = visitor.build_hierarchy();

        let visit_time = start_time.elapsed() - parse_time;
        debug!("[PERF] Document symbols visitor took {:?}", visit_time);

        ruby_symbols
            .iter()
            .map(|symbol| convert_to_document_symbol(symbol.clone()))
            .collect::<Vec<DocumentSymbol>>()
    };
    let project_root = server
        .analysis_workspace_for_uri(&uri)
        .map(|workspace| workspace.root_path);
    match server
        .extension_registry
        .document_symbols_governed(
            server.indexing_resources.clone(),
            project_root,
            uri.as_str().to_string(),
            document.content.clone(),
            server.extension_project_context_for_document(&uri),
        )
        .await
    {
        Ok(extension_symbols) => lsp_symbols.extend(extension_symbols),
        Err(error) => warn!(
            "Extension document-symbol request failed for {}: {error:#}",
            uri.path()
        ),
    }

    debug!("Found {} top-level symbols", lsp_symbols.len());

    info!(
        "[PERF] Document symbols completed in {:?}",
        start_time.elapsed()
    );

    Some(DocumentSymbolResponse::Nested(lsp_symbols))
}

/// Convert internal RubySymbolContext to LSP DocumentSymbol
fn convert_to_document_symbol(ruby_symbol: RubySymbolContext) -> DocumentSymbol {
    // Build detail string with visibility and method kind information
    let detail = build_symbol_detail(&ruby_symbol);

    DocumentSymbol {
        name: ruby_symbol.name,
        detail,
        kind: lsp_symbol_kind(ruby_symbol.kind),
        tags: None,
        #[allow(deprecated)]
        deprecated: None,
        range: lsp_range(ruby_symbol.range),
        selection_range: lsp_range(ruby_symbol.selection_range),
        children: if ruby_symbol.children.is_empty() {
            None
        } else {
            Some(
                ruby_symbol
                    .children
                    .into_iter()
                    .map(convert_to_document_symbol)
                    .collect(),
            )
        },
    }
}

fn lsp_symbol_kind(kind: DocumentSymbolKind) -> tower_lsp::lsp_types::SymbolKind {
    match kind {
        DocumentSymbolKind::Module => tower_lsp::lsp_types::SymbolKind::MODULE,
        DocumentSymbolKind::Class => tower_lsp::lsp_types::SymbolKind::CLASS,
        DocumentSymbolKind::Method => tower_lsp::lsp_types::SymbolKind::METHOD,
        DocumentSymbolKind::Constant => tower_lsp::lsp_types::SymbolKind::CONSTANT,
        DocumentSymbolKind::Property => tower_lsp::lsp_types::SymbolKind::PROPERTY,
    }
}

/// Build detail string for a symbol including visibility and method kind information
fn build_symbol_detail(ruby_symbol: &RubySymbolContext) -> Option<String> {
    let mut detail_parts = Vec::new();

    // Add existing detail if present
    if let Some(existing_detail) = &ruby_symbol.detail {
        detail_parts.push(existing_detail.clone());
    }

    // Add visibility information only for instance methods
    // Singleton methods (class methods) don't follow the same visibility rules in Ruby
    if let Some(visibility) = &ruby_symbol.visibility {
        let is_singleton_method =
            matches!(ruby_symbol.namespace_kind, Some(NamespaceKind::Singleton));

        if !is_singleton_method {
            match visibility {
                MethodVisibility::Private => detail_parts.push("private".to_string()),
                MethodVisibility::Protected => detail_parts.push("protected".to_string()),
                MethodVisibility::Public => {
                    // Only show "public" explicitly for methods to distinguish from default
                    if matches!(ruby_symbol.kind, DocumentSymbolKind::Method) {
                        detail_parts.push("public".to_string());
                    }
                }
            }
        }
    }

    // Add namespace kind information (instance vs singleton/class method)
    if let Some(namespace_kind) = &ruby_symbol.namespace_kind {
        match namespace_kind {
            NamespaceKind::Singleton => detail_parts.push("class method".to_string()),
            NamespaceKind::Instance => detail_parts.push("instance method".to_string()),
        }
    }

    if detail_parts.is_empty() {
        None
    } else {
        Some(detail_parts.join(" • "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruby_analysis::core::{SourcePosition as Position, SourceRange as Range};
    use ruby_analysis::indexer::DocumentSymbolKind as SymbolKind;
    use std::sync::Arc;
    use std::time::Duration;
    use tower_lsp::lsp_types::{
        DidOpenTextDocumentParams, TextDocumentIdentifier, TextDocumentItem, Url,
    };

    fn create_test_range() -> Range {
        Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 10,
            },
        }
    }

    #[test]
    fn test_document_symbol_includes_visibility_information() {
        // Test private method
        let private_method = RubySymbolContext {
            name: "private_method".to_string(),
            kind: SymbolKind::Method,
            detail: None,
            range: create_test_range(),
            selection_range: create_test_range(),
            children: vec![],
            visibility: Some(MethodVisibility::Private),
            namespace_kind: Some(NamespaceKind::Instance),
        };

        let doc_symbol = convert_to_document_symbol(private_method);
        assert_eq!(doc_symbol.name, "private_method");
        assert_eq!(
            doc_symbol.detail,
            Some("private • instance method".to_string())
        );

        // Test class method (should not include visibility)
        let class_method = RubySymbolContext {
            name: "class_method".to_string(),
            kind: SymbolKind::Method,
            detail: None,
            range: create_test_range(),
            selection_range: create_test_range(),
            children: vec![],
            visibility: Some(MethodVisibility::Protected),
            namespace_kind: Some(NamespaceKind::Singleton),
        };

        let doc_symbol = convert_to_document_symbol(class_method);
        assert_eq!(doc_symbol.name, "class_method");
        assert_eq!(doc_symbol.detail, Some("class method".to_string()));

        // Test public method
        let public_method = RubySymbolContext {
            name: "public_method".to_string(),
            kind: SymbolKind::Method,
            detail: None,
            range: create_test_range(),
            selection_range: create_test_range(),
            children: vec![],
            visibility: Some(MethodVisibility::Public),
            namespace_kind: Some(NamespaceKind::Instance),
        };

        let doc_symbol = convert_to_document_symbol(public_method);
        assert_eq!(doc_symbol.name, "public_method");
        assert_eq!(
            doc_symbol.detail,
            Some("public • instance method".to_string())
        );

        // Test protected instance method
        let protected_instance_method = RubySymbolContext {
            name: "protected_instance_method".to_string(),
            kind: SymbolKind::Method,
            detail: None,
            range: create_test_range(),
            selection_range: create_test_range(),
            children: vec![],
            visibility: Some(MethodVisibility::Protected),
            namespace_kind: Some(NamespaceKind::Instance),
        };

        let doc_symbol = convert_to_document_symbol(protected_instance_method);
        assert_eq!(doc_symbol.name, "protected_instance_method");
        assert_eq!(
            doc_symbol.detail,
            Some("protected • instance method".to_string())
        );
    }

    #[test]
    fn test_document_symbol_with_existing_detail() {
        let method_with_detail = RubySymbolContext {
            name: "method_with_signature".to_string(),
            kind: SymbolKind::Method,
            detail: Some("(param1, param2)".to_string()),
            range: create_test_range(),
            selection_range: create_test_range(),
            children: vec![],
            visibility: Some(MethodVisibility::Private),
            namespace_kind: Some(NamespaceKind::Instance),
        };

        let doc_symbol = convert_to_document_symbol(method_with_detail);
        assert_eq!(doc_symbol.name, "method_with_signature");
        assert_eq!(
            doc_symbol.detail,
            Some("(param1, param2) • private • instance method".to_string())
        );
    }

    #[test]
    fn test_document_symbol_non_method_no_visibility() {
        let class_symbol = RubySymbolContext {
            name: "MyClass".to_string(),
            kind: SymbolKind::Class,
            detail: None,
            range: create_test_range(),
            selection_range: create_test_range(),
            children: vec![],
            visibility: None,
            namespace_kind: None,
        };

        let doc_symbol = convert_to_document_symbol(class_symbol);
        assert_eq!(doc_symbol.name, "MyClass");
        assert_eq!(doc_symbol.detail, None);
    }

    #[test]
    fn test_document_symbol_instance_namespace_kind() {
        let method_instance = RubySymbolContext {
            name: "instance_method".to_string(),
            kind: SymbolKind::Method,
            detail: None,
            range: create_test_range(),
            selection_range: create_test_range(),
            children: vec![],
            visibility: Some(MethodVisibility::Private),
            namespace_kind: Some(NamespaceKind::Instance),
        };

        let doc_symbol = convert_to_document_symbol(method_instance);
        assert_eq!(doc_symbol.name, "instance_method");
        assert_eq!(
            doc_symbol.detail,
            Some("private • instance method".to_string())
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn request_time_extension_symbols_wait_for_admission_without_blocking_reactor() {
        let uri =
            Url::parse("file:///tmp/governed_document_symbols.rb").expect("test URI must parse");
        let mut server = RubyLanguageServer::default();
        server.indexing_resources = crate::indexing_resources::IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::with_limits(
                1,
                1,
                256 * 1024 * 1024,
                1,
            ),
        );
        server
            .extension_registry
            .configure_from_config(&crate::config::RubyFastLspConfig {
                extension_packages: vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("extensions/rspec-ruby")
                    .to_string_lossy()
                    .into_owned()],
                ..crate::config::RubyFastLspConfig::default()
            });
        crate::capabilities::indexing::handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "class GovernedDocumentSymbol\nend\n".to_string(),
                },
            },
        )
        .await;

        let holder_release = Arc::new(tokio::sync::Notify::new());
        let holder_release_task = holder_release.clone();
        let holder_governor = server.indexing_resources.clone();
        let holder = tokio::spawn(async move {
            holder_governor
                .run_async_with_resources(
                    "document symbol contention holder",
                    crate::indexing_resources::IndexingWorkSpec::new(
                        None,
                        crate::indexing_resources::IndexingResourcePriority::Background,
                        1,
                        256 * 1024 * 1024,
                        1,
                    ),
                    None,
                    async move {
                        holder_release_task.notified().await;
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
        .expect("resource holder must be admitted before document-symbol request");

        let request_server = server.clone();
        let request_uri = uri.clone();
        let request = tokio::spawn(async move {
            handle_document_symbols(
                &request_server,
                DocumentSymbolParams {
                    text_document: TextDocumentIdentifier { uri: request_uri },
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                },
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while server.indexing_resources.snapshot().queued_tasks != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("document-symbol request must queue behind the complete weighted claim");
        tokio::time::timeout(
            Duration::from_millis(50),
            tokio::time::sleep(Duration::from_millis(1)),
        )
        .await
        .expect("queued document-symbol request must not block the current-thread Tokio reactor");
        assert!(
            !request.is_finished(),
            "document-symbol request must not bypass weighted admission"
        );

        holder_release.notify_one();
        holder.await.unwrap();
        let response = request
            .await
            .unwrap()
            .expect("open document must return symbols");
        let DocumentSymbolResponse::Nested(symbols) = response else {
            panic!(
                "INVARIANT VIOLATED: document-symbol handler returned flat symbols. This is a bug because the handler always constructs a nested hierarchy. Fix: preserve nested document-symbol responses."
            );
        };
        assert!(symbols
            .iter()
            .any(|symbol| symbol.name == "GovernedDocumentSymbol"));
        let complete = server.indexing_resources.snapshot();
        assert_eq!(complete.active_tasks, 0);
        assert_eq!(complete.queued_tasks, 0);
        assert_eq!(complete.completed_tasks, 3);
    }
}
