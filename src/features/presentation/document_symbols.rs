//! Document symbols: the nested class, module, method, and constant outline
//! with visibility details, plus extension symbols.

use log::{debug, info, warn};
use ruby_prism::Visit;
use std::time::Instant;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse};

use crate::server::RubyLanguageServer;
use crate::utils::lsp::lsp_range;

use ruby_analysis::core::NamespaceKind;
use ruby_analysis::indexer::{
    DocumentSymbolKind, DocumentSymbolsVisitor, MethodVisibility, RubySymbolContext,
};

/// Handle `textDocument/documentSymbol`.
pub async fn handle(
    server: &RubyLanguageServer,
    params: DocumentSymbolParams,
) -> LspResult<Option<DocumentSymbolResponse>> {
    let uri = params.text_document.uri;
    let start_time = Instant::now();

    // Get document content from server cache
    let document = match server.get_doc(&uri) {
        Some(doc) => doc,
        None => {
            info!("Document not found in cache for URI: {}", uri);
            return Ok(None);
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
        .extensions
        .registry()
        .document_symbols_governed(
            server.indexing.resources().clone(),
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
    Ok(Some(DocumentSymbolResponse::Nested(lsp_symbols)))
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
}
