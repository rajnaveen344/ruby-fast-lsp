use log::{debug, info};
use ruby_prism::Visit;
use std::time::Instant;
use tower_lsp::lsp_types::{
    DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, SymbolKind,
};

use crate::{
    analyzer_prism::visitors::document_symbols_visitor::DocumentSymbolsVisitor,
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
    DocumentSymbol {
        name: ruby_symbol.name,
        detail: ruby_symbol.detail,
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
    pub visibility: Option<SymbolVisibility>,
    /// Whether it's a class method vs instance method
    pub method_type: Option<MethodType>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SymbolVisibility {
    Public,
    Private,
    Protected,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MethodType {
    Instance,
    Class,
    Singleton,
}