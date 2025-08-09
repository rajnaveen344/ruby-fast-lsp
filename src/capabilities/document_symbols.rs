use log::{debug, info};
use ruby_prism::Visit;
use std::time::Instant;
use tower_lsp::lsp_types::{
    DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, SymbolKind,
};

use crate::{
    analyzer_prism::visitors::document_symbols_visitor::DocumentSymbolsVisitor,
    indexer::entry::{MethodKind, MethodVisibility},
    server::RubyLanguageServer,
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

    // Parse Ruby code using Prism
    let parse_result = ruby_prism::parse(document.content.as_bytes());
    let parse_time = start_time.elapsed();
    debug!("[PERF] Document symbols parse took {:?}", parse_time);

    let root_node = parse_result.node();

    // Extract symbols using visitor
    let mut visitor = DocumentSymbolsVisitor::new(&document);
    visitor.visit(&root_node);
    let ruby_symbols = visitor.build_hierarchy();

    let visit_time = start_time.elapsed() - parse_time;
    debug!("[PERF] Document symbols visitor took {:?}", visit_time);

    // Convert to LSP DocumentSymbol format - all symbols are now top-level since children are nested
    let lsp_symbols: Vec<DocumentSymbol> = ruby_symbols
        .iter()
        .map(|symbol| convert_to_document_symbol(symbol.clone()))
        .collect();

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
        kind: ruby_symbol.kind,
        tags: None,
        deprecated: None,
        range: ruby_symbol.range,
        selection_range: ruby_symbol.selection_range,
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

/// Build detail string for a symbol including visibility and method kind information
fn build_symbol_detail(ruby_symbol: &RubySymbolContext) -> Option<String> {
    let mut detail_parts = Vec::new();
    
    // Add existing detail if present
    if let Some(existing_detail) = &ruby_symbol.detail {
        detail_parts.push(existing_detail.clone());
    }
    
    // Add visibility information only for instance methods
    // Class methods don't follow the same visibility rules in Ruby
    if let Some(visibility) = &ruby_symbol.visibility {
        let is_class_method = matches!(ruby_symbol.method_kind, Some(MethodKind::Class));
        
        if !is_class_method {
            match visibility {
                MethodVisibility::Private => detail_parts.push("private".to_string()),
                MethodVisibility::Protected => detail_parts.push("protected".to_string()),
                MethodVisibility::Public => {
                    // Only show "public" explicitly for methods to distinguish from default
                    if matches!(ruby_symbol.kind, tower_lsp::lsp_types::SymbolKind::METHOD | tower_lsp::lsp_types::SymbolKind::FUNCTION) {
                        detail_parts.push("public".to_string());
                    }
                }
            }
        }
    }
    
    // Add method kind information
    if let Some(method_kind) = &ruby_symbol.method_kind {
        match method_kind {
            MethodKind::Class => detail_parts.push("class method".to_string()),
            MethodKind::Instance => detail_parts.push("instance method".to_string()),
            MethodKind::Unknown => {
                // Don't add any method kind info for unknown methods
            }
        }
    }
    
    if detail_parts.is_empty() {
        None
    } else {
        Some(detail_parts.join(" • "))
    }
}

/// Internal representation of a Ruby symbol with additional context
#[derive(Debug, Clone)]
pub struct RubySymbolContext {
    /// The symbol name
    pub name: String,
    /// Symbol kind (Class, Module, Method, etc.)
    pub kind: SymbolKind,
    /// Additional details (method signature, inheritance, etc.)
    pub detail: Option<String>,
    /// Full range including body
    pub range: tower_lsp::lsp_types::Range,
    /// Selection range (just the name)
    pub selection_range: tower_lsp::lsp_types::Range,
    /// Nested symbols
    pub children: Vec<RubySymbolContext>,
    /// Symbol visibility (public, private, protected)
    pub visibility: Option<MethodVisibility>,
    /// Whether it's a class method vs instance method
    pub method_kind: Option<MethodKind>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Position, Range, SymbolKind};

    fn create_test_range() -> Range {
        Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 0, character: 10 },
        }
    }

    #[test]
    fn test_document_symbol_includes_visibility_information() {
        // Test private method
        let private_method = RubySymbolContext {
            name: "private_method".to_string(),
            kind: SymbolKind::METHOD,
            detail: None,
            range: create_test_range(),
            selection_range: create_test_range(),
            children: vec![],
            visibility: Some(MethodVisibility::Private),
            method_kind: Some(MethodKind::Instance),
        };

        let doc_symbol = convert_to_document_symbol(private_method);
        assert_eq!(doc_symbol.name, "private_method");
        assert_eq!(doc_symbol.detail, Some("private • instance method".to_string()));

        // Test class method (should not include visibility)
        let class_method = RubySymbolContext {
            name: "class_method".to_string(),
            kind: SymbolKind::METHOD,
            detail: None,
            range: create_test_range(),
            selection_range: create_test_range(),
            children: vec![],
            visibility: Some(MethodVisibility::Protected),
            method_kind: Some(MethodKind::Class),
        };

        let doc_symbol = convert_to_document_symbol(class_method);
        assert_eq!(doc_symbol.name, "class_method");
        assert_eq!(doc_symbol.detail, Some("class method".to_string()));

        // Test public method
        let public_method = RubySymbolContext {
            name: "public_method".to_string(),
            kind: SymbolKind::METHOD,
            detail: None,
            range: create_test_range(),
            selection_range: create_test_range(),
            children: vec![],
            visibility: Some(MethodVisibility::Public),
            method_kind: Some(MethodKind::Instance),
        };

        let doc_symbol = convert_to_document_symbol(public_method);
        assert_eq!(doc_symbol.name, "public_method");
        assert_eq!(doc_symbol.detail, Some("public • instance method".to_string()));

        // Test protected instance method
        let protected_instance_method = RubySymbolContext {
            name: "protected_instance_method".to_string(),
            kind: SymbolKind::METHOD,
            detail: None,
            range: create_test_range(),
            selection_range: create_test_range(),
            children: vec![],
            visibility: Some(MethodVisibility::Protected),
            method_kind: Some(MethodKind::Instance),
        };

        let doc_symbol = convert_to_document_symbol(protected_instance_method);
        assert_eq!(doc_symbol.name, "protected_instance_method");
        assert_eq!(doc_symbol.detail, Some("protected • instance method".to_string()));
    }

    #[test]
    fn test_document_symbol_with_existing_detail() {
        let method_with_detail = RubySymbolContext {
            name: "method_with_signature".to_string(),
            kind: SymbolKind::METHOD,
            detail: Some("(param1, param2)".to_string()),
            range: create_test_range(),
            selection_range: create_test_range(),
            children: vec![],
            visibility: Some(MethodVisibility::Private),
            method_kind: Some(MethodKind::Instance),
        };

        let doc_symbol = convert_to_document_symbol(method_with_detail);
        assert_eq!(doc_symbol.name, "method_with_signature");
        assert_eq!(doc_symbol.detail, Some("(param1, param2) • private • instance method".to_string()));
    }

    #[test]
    fn test_document_symbol_non_method_no_visibility() {
        let class_symbol = RubySymbolContext {
            name: "MyClass".to_string(),
            kind: SymbolKind::CLASS,
            detail: None,
            range: create_test_range(),
            selection_range: create_test_range(),
            children: vec![],
            visibility: None,
            method_kind: None,
        };

        let doc_symbol = convert_to_document_symbol(class_symbol);
        assert_eq!(doc_symbol.name, "MyClass");
        assert_eq!(doc_symbol.detail, None);
    }

    #[test]
    fn test_document_symbol_unknown_method_kind() {
        let method_unknown = RubySymbolContext {
            name: "unknown_method".to_string(),
            kind: SymbolKind::METHOD,
            detail: None,
            range: create_test_range(),
            selection_range: create_test_range(),
            children: vec![],
            visibility: Some(MethodVisibility::Private),
            method_kind: Some(MethodKind::Unknown),
        };

        let doc_symbol = convert_to_document_symbol(method_unknown);
        assert_eq!(doc_symbol.name, "unknown_method");
        assert_eq!(doc_symbol.detail, Some("private".to_string()));
    }
}
