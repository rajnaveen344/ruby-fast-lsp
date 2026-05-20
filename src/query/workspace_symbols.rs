//! Workspace Symbol Query — LSP adapter over analysis-engine symbol search.

use ruby_analysis_core::SymbolKind as AnalysisSymbolKind;
use ruby_analysis_engine::AnalysisQuery;
use tower_lsp::lsp_types::{SymbolInformation, SymbolKind};

use super::analysis_location::location_for_range;
use super::EngineQuery;

impl EngineQuery {
    pub fn has_analysis_symbols(&self) -> bool {
        let Some(engine) = self.analysis_engine() else {
            return false;
        };
        let engine = engine.lock();
        AnalysisQuery::new(&engine).has_symbols()
    }

    pub fn get_top_level_symbols(&self) -> Vec<SymbolInformation> {
        let engine_ref = self.analysis_engine().expect(
            "INVARIANT VIOLATED: workspace symbols query requires an analysis engine. \
             This is a bug because LSP workspace/symbol should be a thin wrapper over AnalysisEngine. \
             Fix: construct EngineQuery with with_engine().",
        );
        let engine = engine_ref.lock();
        AnalysisQuery::new(&engine)
            .top_level_symbols(50)
            .into_iter()
            .filter_map(|symbol| symbol_information_from_engine_symbol(&engine, symbol))
            .collect()
    }

    pub fn search_workspace_symbols(&self, query: &str) -> Vec<SymbolInformation> {
        let engine_ref = self.analysis_engine().expect(
            "INVARIANT VIOLATED: workspace symbol search requires an analysis engine. \
             This is a bug because LSP workspace/symbol should be a thin wrapper over AnalysisEngine. \
             Fix: construct EngineQuery with with_engine().",
        );
        let engine = engine_ref.lock();
        AnalysisQuery::new(&engine)
            .search_workspace_symbols(query, 100)
            .into_iter()
            .filter_map(|symbol| symbol_information_from_engine_symbol(&engine, symbol))
            .collect()
    }
}

fn symbol_information_from_engine_symbol(
    engine: &ruby_analysis_engine::AnalysisEngine,
    symbol: ruby_analysis_engine::WorkspaceSymbolMatch,
) -> Option<SymbolInformation> {
    Some(SymbolInformation {
        name: symbol.name,
        kind: analysis_symbol_kind_to_lsp_kind(symbol.kind),
        tags: None,
        #[allow(deprecated)]
        deprecated: Some(false),
        location: location_for_range(engine, symbol.range)?,
        container_name: symbol.container_name,
    })
}

fn analysis_symbol_kind_to_lsp_kind(kind: AnalysisSymbolKind) -> SymbolKind {
    match kind {
        AnalysisSymbolKind::Class => SymbolKind::CLASS,
        AnalysisSymbolKind::Module => SymbolKind::MODULE,
        AnalysisSymbolKind::Method => SymbolKind::METHOD,
        AnalysisSymbolKind::Constant => SymbolKind::CONSTANT,
        AnalysisSymbolKind::LocalVariable
        | AnalysisSymbolKind::InstanceVariable
        | AnalysisSymbolKind::ClassVariable
        | AnalysisSymbolKind::GlobalVariable => SymbolKind::VARIABLE,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::Mutex;
    use ruby_analysis_core::{
        FullyQualifiedName, RubyConstant, RubyMethod, SourceFileId, SymbolFact,
        SymbolKind as AnalysisSymbolKind, TextRange,
    };
    use ruby_analysis_engine::AnalysisEngine;

    use super::*;

    fn query_with_analysis_symbols() -> EngineQuery {
        let source = "class User\n  def name\n  end\nend";
        let mut engine = AnalysisEngine::new();
        let file_id = engine.open_or_update_file("/tmp/user.rb", source);
        assert_eq!(
            file_id,
            SourceFileId(0),
            "INVARIANT VIOLATED: first test analysis file id changed. \
             This is a bug because this test assumes a fresh AnalysisEngine. \
             Fix: update the expected file id or avoid asserting it."
        );

        let user = RubyConstant::new("User").expect("test constant must be valid");
        engine.add_symbol_fact(SymbolFact::new(
            FullyQualifiedName::namespace(vec![user.clone()]),
            AnalysisSymbolKind::Class,
            TextRange::new(file_id, 6, 10),
        ));
        engine.add_symbol_fact(SymbolFact::new(
            FullyQualifiedName::method(
                vec![user],
                RubyMethod::new("name").expect("test method must be valid"),
            ),
            AnalysisSymbolKind::Method,
            TextRange::new(file_id, 17, 21),
        ));

        EngineQuery::with_engine(Arc::new(Mutex::new(engine)))
    }

    #[test]
    fn workspace_symbols_can_read_analysis_engine_without_index_entries() {
        let query = query_with_analysis_symbols();

        let symbols = query.search_workspace_symbols("name");

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "name");
        assert_eq!(symbols[0].kind, SymbolKind::METHOD);
        assert_eq!(symbols[0].container_name.as_deref(), Some("User"));
    }

    #[test]
    fn top_level_symbols_can_read_analysis_engine_without_index_entries() {
        let query = query_with_analysis_symbols();

        let symbols = query.get_top_level_symbols();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].kind, SymbolKind::CLASS);
    }
}
