//! Document lifecycle and watched-file indexing tests.

use ruby_analysis::core::{
    FileAnalysis, FullyQualifiedName, GraphEdgeKind, MethodFact, NamespaceKind, RubyConstant,
    RubyMethod, SymbolKind, TextRange,
};
use ruby_analysis::engine::ResolveMode;
use tower_lsp::LanguageServer;

use super::*;

fn namespace(name: &str) -> FullyQualifiedName {
    FullyQualifiedName::namespace(vec![
        RubyConstant::new(name).expect("test namespace must be valid")
    ])
}

fn has_namespace(server: &RubyLanguageServer, uri: &Url, name: &str) -> bool {
    let project = server.project_for_uri(uri);
    let engine = project.test_read();
    !engine.view().symbol_facts_for(&namespace(name)).is_empty()
}

mod document_lifecycle;
mod watched_files;
