//! Unit tests for the AST visitors found in `src/analyzer_prism/visitors/index_visitor`.
//! Initially we focus on the `class_node` visitor so that we can iron-out the
//! test harness and conventions.
//!
//! NOTE: Prism nodes cannot be built manually; they must be produced by parsing
//! real Ruby source.  Therefore each test feeds a Ruby snippet to the Prism
//! parser, runs the `IndexVisitor`, and then inspects the resulting `RubyIndex`
//! and `ScopeTracker` state.

use crate::{
    analyzer_prism::visitors::index_visitor::IndexVisitor, server::RubyLanguageServer,
    types::ruby_document::RubyDocument,
};
use parking_lot::RwLock;
use ruby_prism::Visit;
use std::sync::Arc;
use tower_lsp::lsp_types::Url;

mod class_node_test;
mod def_node_test;
mod module_node_test;

/// Helper that parses `code`, runs the full `IndexVisitor`, and returns the
/// visitor so that callers can inspect its public fields (index & scope).
pub fn visit_code(code: &str) -> IndexVisitor {
    // Create an in-memory LSP URI so the server can track the document.
    let uri = Url::parse("file:///__virtual__/snippet.rb").unwrap();

    // Stand-alone language server instance (no client needed for unit tests).
    let server = RubyLanguageServer::default();

    // Insert a `RubyDocument` so that `IndexVisitor::new` can retrieve it.
    let doc = RubyDocument::new(uri.clone(), code.to_string(), 0);
    server
        .docs
        .lock()
        .insert(uri.clone(), Arc::new(RwLock::new(doc.clone())));

    // Parse with Prism and run the visitor.
    let doc = doc;
    let parse_result = doc.parse();
    let mut visitor = IndexVisitor::new(server.index(), doc.clone());
    visitor.visit(&parse_result.node());

    visitor
}
