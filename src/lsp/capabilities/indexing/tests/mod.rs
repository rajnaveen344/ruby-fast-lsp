//! Document lifecycle and watched-file indexing tests.

use ruby_analysis::core::{
    FileAnalysis, FullyQualifiedName, GraphEdgeKind, MethodFact, NamespaceKind, RubyConstant,
    RubyMethod, SymbolKind, TextRange,
};
use ruby_analysis::engine::{AnalysisQuery, ResolveMode};
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

mod document_lifecycle;
mod watched_files;
